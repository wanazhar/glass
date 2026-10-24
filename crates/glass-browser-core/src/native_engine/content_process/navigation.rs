use super::*;

/// Opt-in, privacy-safe timings for the content-process load pipeline. The
/// trace intentionally records only the local IPC request number, fixed stage
/// names, item counts and elapsed time; it never records a URL, response body,
/// cookie, selector or script text. Keeping this inside the child process lets
/// us distinguish network waits from native parsing without routing diagnostics
/// through the interactive TUI.
fn trace_content_load_stage(
    request_id: u64,
    stage: &'static str,
    phase: &'static str,
    elapsed_us: Option<u128>,
    items: Option<usize>,
    ordinal: Option<usize>,
    resource_type: Option<&'static str>,
) {
    let Some(path) = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE") else {
        return;
    };
    let path = std::path::PathBuf::from(path).with_extension("native_load.jsonl");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = file.metadata() {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = file.set_permissions(permissions);
        }
    }
    let mut record = serde_json::json!({
        "component": "native_load",
        "process_id": std::process::id(),
        "request_id": request_id,
        "stage": stage,
        "phase": phase,
        "elapsed_us": elapsed_us,
    });
    if let Some(items) = items {
        record["items"] = serde_json::json!(items);
    }
    if let Some(ordinal) = ordinal {
        record["ordinal"] = serde_json::json!(ordinal);
    }
    if let Some(resource_type) = resource_type {
        record["resource_type"] = serde_json::json!(resource_type);
    }
    if let Ok(mut bytes) = serde_json::to_vec(&record) {
        bytes.push(b'\n');
        let _ = std::io::Write::write_all(&mut file, &bytes);
    }
}

pub(crate) struct ContentLoadStage {
    request_id: u64,
    name: &'static str,
    started: std::time::Instant,
    completed: bool,
    ordinal: Option<usize>,
    resource_type: Option<&'static str>,
}

impl ContentLoadStage {
    pub(crate) fn begin(request_id: u64, name: &'static str) -> Self {
        Self::begin_with_resource(request_id, name, None, None)
    }

    fn begin_resource(request_id: u64, resource_type: &'static str, ordinal: usize) -> Self {
        Self::begin_with_resource(
            request_id,
            "resource_fetch",
            Some(ordinal),
            Some(resource_type),
        )
    }

    pub(crate) fn begin_with_resource(
        request_id: u64,
        name: &'static str,
        ordinal: Option<usize>,
        resource_type: Option<&'static str>,
    ) -> Self {
        trace_content_load_stage(
            request_id,
            name,
            "start",
            None,
            None,
            ordinal,
            resource_type,
        );
        Self {
            request_id,
            name,
            started: std::time::Instant::now(),
            completed: false,
            ordinal,
            resource_type,
        }
    }

    pub(crate) fn finish(self, items: Option<usize>) {
        self.finish_as("complete", items);
    }

    fn finish_as(mut self, phase: &'static str, items: Option<usize>) {
        trace_content_load_stage(
            self.request_id,
            self.name,
            phase,
            Some(self.started.elapsed().as_micros()),
            items,
            self.ordinal,
            self.resource_type,
        );
        self.completed = true;
    }
}

impl Drop for ContentLoadStage {
    fn drop(&mut self) {
        if !self.completed {
            trace_content_load_stage(
                self.request_id,
                self.name,
                "incomplete",
                Some(self.started.elapsed().as_micros()),
                None,
                self.ordinal,
                self.resource_type,
            );
        }
    }
}

pub(super) async fn load_parent_content_navigation(
    request_id: u64,
    generation: u32,
    context_id: &str,
    frame_id: &str,
    navigation: &NativeNavigationRequest,
    stdout: &mut (impl AsyncWrite + Unpin),
    ipc_requests: &mut mpsc::Receiver<Result<Vec<u8>, NativeEngineError>>,
    cancelled_parent_fetches: &mut BTreeSet<(u64, u32)>,
    loader: &mut NativeResourceLoader,
    document_cookie_projection: &mut Option<String>,
) -> Result<NativeResource, NativeEngineError> {
    if !is_network_url(without_fragment(&navigation.url)) {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "the parent navigation broker accepts only HTTP(S) URLs".into(),
        });
    }
    write_value_frame(
        stdout,
        &json!({
            "kind": "parent_navigation_request",
            "id": request_id,
            "generation": generation,
            "context_id": context_id,
            "frame_id": frame_id,
            "url": navigation.url,
        }),
    )
    .await?;
    let payload = receive_content_ipc_response(
        ipc_requests,
        cancelled_parent_fetches,
        "parent navigation broker",
    )
    .await?;
    let response: Value = serde_json::from_slice(&payload).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent navigation response",
            NativeWorkerFailureKind::Protocol,
            "parent returned invalid navigation JSON",
        )
    })?;
    if response.get("kind").and_then(Value::as_str) == Some("error") {
        return Err(NativeEngineError::Network {
            operation: "parent-brokered navigation".into(),
            reason: response
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("parent rejected the document navigation")
                .to_owned(),
        });
    }
    if response.get("kind").and_then(Value::as_str) != Some("parent_navigation_response")
        || response.get("id").and_then(Value::as_u64) != Some(request_id)
        || response.get("generation").and_then(Value::as_u64) != Some(u64::from(generation))
    {
        return Err(NativeEngineError::worker_failure(
            "decode parent navigation response",
            NativeWorkerFailureKind::Protocol,
            "parent response does not match the pending document navigation",
        ));
    }
    let url = response.get("url").and_then(Value::as_str).ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent navigation response",
            NativeWorkerFailureKind::Protocol,
            "parent omitted the final document URL",
        )
    })?;
    validate_url_text("parent navigation final URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::worker_failure(
            "decode parent navigation response",
            NativeWorkerFailureKind::Protocol,
            "parent returned a non-HTTP(S) document URL",
        ));
    }
    let encoded_body = response
        .get("body_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent navigation response",
                NativeWorkerFailureKind::Protocol,
                "parent omitted the bounded document body",
            )
        })?;
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded_body)
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent navigation response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an invalid document body encoding",
            )
        })?;
    if body.len() > loader.max_document_bytes() {
        return Err(NativeEngineError::limit(
            "parent-brokered navigation document",
            loader.max_document_bytes(),
            body.len(),
        ));
    }
    let body = String::from_utf8(body).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent navigation response",
            NativeWorkerFailureKind::Protocol,
            "parent returned a non-UTF-8 decoded document",
        )
    })?;
    let document_cookie = response
        .get("document_cookie")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent navigation response",
                NativeWorkerFailureKind::Protocol,
                "parent omitted the visible document.cookie projection",
            )
        })?;
    if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
        return Err(NativeEngineError::limit(
            "parent document.cookie projection",
            MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
            document_cookie.len(),
        ));
    }
    let policy_headers = response
        .get("response_policy_headers")
        .cloned()
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent navigation response",
                NativeWorkerFailureKind::Protocol,
                "parent omitted response policy metadata",
            )
        })
        .and_then(|value| {
            serde_json::from_value::<Vec<(String, String)>>(value).map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent navigation response",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned malformed response policy metadata",
                )
            })
        })?;
    loader.set_document_content_security_policy_from_pairs(url, &policy_headers)?;
    *document_cookie_projection = Some(document_cookie.to_owned());
    let parsed_url = Url::parse(without_fragment(url)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent navigation response",
            NativeWorkerFailureKind::Protocol,
            "parent returned invalid final URL syntax",
        )
    })?;
    Ok(NativeResource {
        url: url.to_owned(),
        origin: NativeOrigin::from_url(&parsed_url)?,
        body,
    })
}

pub(super) fn decode_content_process_load_navigation(
    request: &Value,
) -> Result<NativeNavigationRequest, NativeEngineError> {
    let url = request
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::invalid("content-process URL", "must be text"))?;
    validate_url_text("content-process URL", url)?;
    if request
        .get("object_url")
        .is_some_and(|value| !value.is_null())
    {
        return Err(NativeEngineError::worker_failure(
            "content process parent navigation broker",
            NativeWorkerFailureKind::Protocol,
            "an object URL cannot use the HTTP(S) parent navigation broker",
        ));
    }
    let method = match request.get("method").and_then(Value::as_str) {
        Some("GET") => NativeNavigationMethod::Get,
        Some("POST") => NativeNavigationMethod::Post,
        _ => {
            return Err(NativeEngineError::invalid(
                "content-process navigation method",
                "must be GET or POST",
            ));
        }
    };
    let text_body = request
        .get("body")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation body",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    let binary_body = request
        .get("body_base64")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation binary body",
                    "must be base64 text or null",
                )
            })
        })
        .transpose()?;
    if text_body.is_some() && binary_body.is_some() {
        return Err(NativeEngineError::invalid(
            "content-process navigation body",
            "must use either text or base64 bytes",
        ));
    }
    let body = match binary_body {
        Some(encoded) => Some(NativeRequestBody::Bytes(
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| {
                    NativeEngineError::invalid(
                        "content-process navigation binary body",
                        "must be valid base64",
                    )
                })?,
        )),
        None => text_body.map(str::to_owned).map(NativeRequestBody::Text),
    };
    let content_type = request
        .get("content_type")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation content type",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    match method {
        NativeNavigationMethod::Get if body.is_none() && content_type.is_none() => {
            Ok(NativeNavigationRequest::get(url))
        }
        NativeNavigationMethod::Post => NativeNavigationRequest::post_with_body(
            url,
            body.ok_or_else(|| {
                NativeEngineError::invalid("content-process POST body", "must be present")
            })?,
            content_type
                .unwrap_or("application/x-www-form-urlencoded")
                .to_owned(),
        ),
        NativeNavigationMethod::Get => Err(NativeEngineError::invalid(
            "content-process GET body",
            "must be absent",
        )),
        _ => Err(NativeEngineError::invalid(
            "content-process navigation method",
            "must be GET or POST",
        )),
    }
}

pub(super) async fn load_content_resource(
    request_id: u64,
    request: &Value,
    resource_loader: &mut Option<NativeResourceLoader>,
    storage_path: Option<&Path>,
    allowed_file_roots: &[PathBuf],
    environment: &NativeEnvironmentOverrides,
    service_workers: &mut NativeServiceWorkerRegistry,
    resumed_fetch: Option<NativeServiceWorkerFetchCompletion>,
    context_id: &str,
    frame_id: &str,
    stdout: &mut tokio::io::Stdout,
    ipc_requests: &mut mpsc::Receiver<Result<Vec<u8>, NativeEngineError>>,
    cancelled_parent_fetches: &mut BTreeSet<(u64, u32)>,
    parent_document_cookie_projection: &mut Option<String>,
) -> Result<
    Option<(
        NativeContentLoad,
        NativeDocument,
        Viewport,
        Vec<NativePageScript>,
        Vec<(u32, NativeEventKind)>,
    )>,
    NativeEngineError,
> {
    let url = request
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::invalid("content-process URL", "must be text"))?;
    validate_url_text("content-process URL", url)?;
    let generation = request
        .get("generation")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|generation| *generation != 0)
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "content-process document generation",
                "must be a positive integer",
            )
        })?;
    let object_url = match request.get("object_url") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            serde_json::from_value::<NativeObjectUrlResource>(value.clone()).map_err(|_| {
                NativeEngineError::invalid(
                    "content-process object URL payload",
                    "must be a valid bounded native object URL resource",
                )
            })?,
        ),
    };
    if object_url
        .as_ref()
        .is_some_and(|resource| resource.body.len() > MAX_NATIVE_SCRIPT_BYTES)
    {
        return Err(NativeEngineError::limit(
            "content-process object URL payload",
            MAX_NATIVE_SCRIPT_BYTES,
            object_url
                .as_ref()
                .map_or(0, |resource| resource.body.len()),
        ));
    }
    if !is_network_url(without_fragment(url))
        && !is_file_url(without_fragment(url))
        && object_url.is_none()
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "content process accepts only HTTP(S) or rooted file document URLs".into(),
        });
    }
    let method = match request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
    {
        "GET" => NativeNavigationMethod::Get,
        "POST" => NativeNavigationMethod::Post,
        _ => {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "content process supports only GET and POST document navigation".into(),
            });
        }
    };
    let text_body = request
        .get("body")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation body",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    let body_base64 = request
        .get("body_base64")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation binary body",
                    "must be base64 text or null",
                )
            })
        })
        .transpose()?;
    if text_body.is_some() && body_base64.is_some() {
        return Err(NativeEngineError::invalid(
            "content-process navigation body",
            "must use either text or base64 bytes",
        ));
    }
    let body = match body_base64 {
        Some(encoded) => Some(NativeRequestBody::Bytes(
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| {
                    NativeEngineError::invalid(
                        "content-process navigation binary body",
                        "must be valid base64",
                    )
                })?,
        )),
        None => text_body.map(str::to_owned).map(NativeRequestBody::Text),
    };
    let content_type = request
        .get("content_type")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation content type",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    let mut navigation = match method {
        NativeNavigationMethod::Get => {
            if body.is_some() {
                return Err(NativeEngineError::invalid(
                    "content-process GET body",
                    "must be null",
                ));
            }
            if content_type.is_some() {
                return Err(NativeEngineError::invalid(
                    "content-process GET content type",
                    "must be null",
                ));
            }
            NativeNavigationRequest::get(url)
        }
        NativeNavigationMethod::Post => NativeNavigationRequest::post_with_body(
            url,
            body.ok_or_else(|| {
                NativeEngineError::invalid("content-process POST body", "must be present")
            })?,
            content_type
                .unwrap_or("application/x-www-form-urlencoded")
                .to_owned(),
        )?,
        _ => {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "content process supports only GET and POST document navigation".into(),
            });
        }
    };
    navigation.object_url = object_url;
    let max_document_bytes = request
        .get("max_document_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "content-process document limit",
                "must be a positive integer",
            )
        })?;
    let limits = NativeEngineLimits {
        max_document_bytes,
        max_nodes: request
            .get("max_nodes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_nodes),
        max_dom_depth: request
            .get("max_dom_depth")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_dom_depth),
        max_text_bytes: request
            .get("max_text_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_text_bytes),
        ..NativeEngineLimits::default()
    };
    let viewport = Viewport {
        width: request
            .get("viewport_width")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().width),
        height: request
            .get("viewport_height")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().height),
        device_scale_factor_milli: request
            .get("viewport_device_scale_factor_milli")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().device_scale_factor_milli),
    };
    viewport.validate()?;
    let referrer = request
        .get("referrer")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid("content-process referrer", "must be text or null")
            })
        })
        .transpose()?;
    let referrer_policy = request
        .get("referrer_policy")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "content-process referrer policy",
                "must be a supported Referrer-Policy token",
            )
        })
        .and_then(NativeFetchReferrerPolicy::parse)?;
    let client_id = request.get("client_id").and_then(Value::as_str);
    if let Some(value) = request.get("service_worker_clients") {
        let clients: Vec<NativeServiceWorkerClientState> = serde_json::from_value(value.clone())
            .map_err(|_| {
                NativeEngineError::invalid(
                    "content-process Service Worker clients",
                    "must be a valid native client projection",
                )
            })?;
        service_workers.replace_client_states(clients)?;
    }
    let loader = match resource_loader {
        Some(loader) if loader.max_document_bytes() == max_document_bytes => loader,
        Some(_) => {
            return Err(NativeEngineError::invalid(
                "content-process document limit",
                "cannot change the document limit after the content process starts",
            ));
        }
        None => {
            *resource_loader = Some(NativeResourceLoader::for_content_process(
                max_document_bytes,
                storage_path,
                allowed_file_roots,
            )?);
            resource_loader
                .as_mut()
                .expect("content-process resource loader was just initialized")
        }
    };
    loader.set_environment(environment)?;
    let response_stage = ContentLoadStage::begin(request_id, "navigation_response");
    let resource = if navigation.object_url.is_some() {
        loader
            .load_async_request_with_referrer_policy(&navigation, referrer, referrer_policy)
            .await?
    } else if let Some(completion) = resumed_fetch {
        match completion.response {
            Some(response) => {
                let response_headers = response.headers.clone();
                let resource = service_worker_fetch_resource(response, url)?;
                loader.set_document_content_security_policy_from_pairs(
                    &resource.url,
                    &response_headers,
                )?;
                resource
            }
            None => {
                if is_network_url(without_fragment(&navigation.url)) {
                    load_parent_content_navigation(
                        request_id,
                        generation,
                        context_id,
                        frame_id,
                        &navigation,
                        stdout,
                        ipc_requests,
                        cancelled_parent_fetches,
                        loader,
                        parent_document_cookie_projection,
                    )
                    .await?
                } else {
                    loader
                        .load_async_request_with_referrer_policy(
                            &navigation,
                            referrer,
                            referrer_policy,
                        )
                        .await?
                }
            }
        }
    } else {
        service_workers.begin_document(url, client_id)?;
        let mut navigation_parent_fetch_broker =
            is_network_url(without_fragment(url)).then(|| NativeContentFetchBroker {
                captured_load_fetches: false,
                request_id,
                owner: NativeContentCookieOwner {
                    context_id: context_id.to_owned(),
                    frame_id: frame_id.to_owned(),
                    generation,
                    document_url: url.to_owned(),
                },
                runtime: None,
                stdout,
                ipc_requests,
                cancelled_parent_fetches,
                document_cookie_projection: parent_document_cookie_projection,
                next_content_resource_fetch_id: 0,
                page_meta_content_security_policies: Vec::new(),
            });
        if let Some(parent_fetch_broker) = navigation_parent_fetch_broker.as_mut() {
            service_workers
                .restore_for_document(url, loader, generation, Some(parent_fetch_broker))
                .await?;
        } else {
            service_workers
                .restore_for_document(url, loader, generation, None)
                .await?;
        }
        match service_workers
            .intercept_navigation(
                loader,
                &navigation,
                referrer,
                referrer_policy,
                navigation_parent_fetch_broker.as_mut(),
            )
            .await?
        {
            NativeServiceWorkerNavigationOutcome::Handled(resource) => resource,
            NativeServiceWorkerNavigationOutcome::NotHandled => {
                if is_network_url(without_fragment(&navigation.url)) {
                    load_parent_content_navigation(
                        request_id,
                        generation,
                        context_id,
                        frame_id,
                        &navigation,
                        stdout,
                        ipc_requests,
                        cancelled_parent_fetches,
                        loader,
                        parent_document_cookie_projection,
                    )
                    .await?
                } else {
                    loader
                        .load_async_request_with_referrer_policy(
                            &navigation,
                            referrer,
                            referrer_policy,
                        )
                        .await?
                }
            }
            NativeServiceWorkerNavigationOutcome::Suspended => {
                response_stage.finish_as("suspended", None);
                return Ok(None);
            }
        }
    };
    response_stage.finish(None);
    if navigation.object_url.is_none() {
        service_workers.commit_document(&resource.url)?;
    }
    let owner = NativeContentCookieOwner {
        context_id: context_id.to_owned(),
        frame_id: frame_id.to_owned(),
        generation,
        document_url: resource.url.clone(),
    };
    let mut parent_fetch_broker = NativeContentFetchBroker {
        captured_load_fetches: false,
        request_id,
        owner,
        runtime: None,
        stdout,
        ipc_requests,
        cancelled_parent_fetches,
        document_cookie_projection: parent_document_cookie_projection,
        next_content_resource_fetch_id: 0,
        page_meta_content_security_policies: Vec::new(),
    };
    let discovery_stage = ContentLoadStage::begin(request_id, "document_discovery_parse");
    let mut discovery = NativeDocument::parse_with_generation_and_referrer_policy(
        &resource.body,
        &limits,
        generation,
        loader.document_referrer_policy(&resource.url)?,
    )?;
    let stylesheet_links = discovery.external_stylesheet_links();
    discovery_stage.finish(Some(stylesheet_links.len()));
    loader.apply_meta_content_security_policies(
        &resource.url,
        &discovery.content_security_policy_meta(),
    )?;
    parent_fetch_broker.page_meta_content_security_policies =
        loader.document_meta_content_security_policies(&resource.url)?;
    let frame_sources = loader.frame_sources_for_document(&resource.url)?;
    let navigate_to_sources = loader
        .navigation_sources_for_document(&resource.url, NativeNavigationPolicyKind::NavigateTo)?;
    let allowed_inline_style_nodes =
        inline_style_policy_nodes(&mut discovery, loader, &resource.url)?;
    let mut external_stylesheet_states = Vec::new();
    let mut external_stylesheet_bytes = 0usize;
    let mut resource_events = Vec::new();
    let stylesheet_stage = ContentLoadStage::begin(request_id, "stylesheet_fetches");
    for (ordinal, href) in stylesheet_links
        .into_iter()
        .take(MAX_CONTENT_STYLESHEETS)
        .enumerate()
    {
        let (node_index, href, integrity, crossorigin) = href;
        let referrer_policy = discovery.stylesheet_link_referrer_policy_for_node_index(node_index);
        let resource_stage = ContentLoadStage::begin_resource(request_id, "stylesheet", ordinal);
        let stylesheet_result = if is_parent_owned_network_target(&resource.url, &href) {
            parent_fetch_broker
                .load_stylesheet(
                    &resource.url,
                    &href,
                    integrity.as_deref(),
                    crossorigin.as_deref(),
                    Some(referrer_policy),
                )
                .await
        } else {
            loader
                .load_stylesheet_async_with_object_url_and_referrer_policy(
                    &resource.url,
                    &href,
                    integrity.as_deref(),
                    crossorigin.as_deref(),
                    None,
                    Some(referrer_policy),
                )
                .await
        };
        let body = match stylesheet_result {
            Ok(Some(stylesheet)) => {
                if is_network_url(&stylesheet.url) {
                    expand_network_stylesheet_imports(
                        loader,
                        &resource.url,
                        stylesheet,
                        viewport,
                        &mut external_stylesheet_bytes,
                        Some(&mut parent_fetch_broker),
                    )
                    .await
                    .ok()
                } else {
                    let next_len = external_stylesheet_bytes.saturating_add(stylesheet.body.len());
                    if next_len <= MAX_CONTENT_STYLESHEET_BYTES {
                        external_stylesheet_bytes = next_len;
                        Some(stylesheet)
                    } else {
                        None
                    }
                }
            }
            Ok(None) | Err(_) => None,
        };
        resource_stage.finish(None);
        let event_kind = body
            .as_ref()
            .map_or(NativeEventKind::Error, |_| NativeEventKind::Load);
        external_stylesheet_states.push((
            node_index,
            href,
            body.as_ref().map(|stylesheet| stylesheet.url.clone()),
            body.map(|stylesheet| stylesheet.body),
        ));
        resource_events.push((node_index, event_kind));
    }
    stylesheet_stage.finish(Some(external_stylesheet_states.len()));
    let external_stylesheets = external_stylesheet_states
        .iter()
        .filter_map(|(_, href, stylesheet_url, body)| {
            body.as_ref().map(|body| {
                let stylesheet_url = stylesheet_url.as_deref().unwrap_or(href);
                absolutize_stylesheet_urls(body, &resource.url, stylesheet_url)
            })
        })
        .collect::<Vec<_>>();
    let document_parse_stage = ContentLoadStage::begin(request_id, "document_stylesheet_parse");
    let mut document = NativeDocument::parse_with_stylesheets_inline_policy_and_referrer_policy(
        &resource.body,
        &limits,
        &external_stylesheets,
        generation,
        Some(&allowed_inline_style_nodes),
        loader.document_referrer_policy(&resource.url)?,
    )?;
    document_parse_stage.finish(Some(external_stylesheets.len()));
    document.set_css_target_from_url(&resource.url)?;
    document.set_viewport(viewport)?;
    document.set_external_stylesheet_states(external_stylesheet_states);
    document.mark_inline_style_reports_seen();
    document.mark_content_security_policy_meta_processed();
    let font_stage = ContentLoadStage::begin(request_id, "font_faces");
    load_font_faces(
        &mut document,
        None,
        loader,
        &resource.url,
        Some(&mut parent_fetch_broker),
    )
    .await?;
    font_stage.finish(None);
    let image_preload_stage = ContentLoadStage::begin(request_id, "image_preloads");
    let image_preload_events = load_external_image_preloads(
        &mut document,
        loader,
        &resource.url,
        Some(&mut parent_fetch_broker),
        Some(request_id),
    )
    .await?;
    let image_preload_count = image_preload_events.len();
    resource_events.extend(image_preload_events);
    image_preload_stage.finish(Some(image_preload_count));
    let image_stage = ContentLoadStage::begin(request_id, "images");
    let image_events = load_external_images(
        &mut document,
        None,
        loader,
        &resource.url,
        viewport,
        Some(&mut parent_fetch_broker),
        Some(request_id),
    )
    .await?;
    resource_events.extend(image_events);
    // This stage includes CSS background images, which do not produce load
    // events in the same vector. Per-resource trace rows provide the exact
    // counts without overstating this event-vector length.
    image_stage.finish(None);
    let poster_stage = ContentLoadStage::begin(request_id, "video_posters");
    load_external_video_posters(
        &mut document,
        loader,
        &resource.url,
        Some(&mut parent_fetch_broker),
        Some(request_id),
    )
    .await?;
    poster_stage.finish(None);
    let media_stage = ContentLoadStage::begin(request_id, "media");
    let media_events = load_external_media(
        &mut document,
        None,
        loader,
        &resource.url,
        Some(&mut parent_fetch_broker),
        Some(request_id),
    )
    .await?;
    let media_event_count = media_events.len();
    resource_events.extend(media_events);
    media_stage.finish(Some(media_event_count));
    let script_stage = ContentLoadStage::begin(request_id, "script_sources");
    let (script_sources, script_resource_events) = load_page_script_sources(
        &document,
        loader,
        &resource.url,
        Some(&mut parent_fetch_broker),
        Some(request_id),
    )
    .await?;
    script_stage.finish(None);
    resource_events.extend(script_resource_events);
    resource_events.sort_unstable_by_key(|(node_index, _)| *node_index);
    resource_events.dedup();
    let wire_stage = ContentLoadStage::begin(request_id, "document_wire_encode");
    let wire = document.to_content_wire();
    wire_stage.finish(Some(wire.nodes.len()));
    Ok(Some((
        NativeContentLoad {
            url: resource.url,
            generation,
            origin: resource.origin,
            document: wire,
            frame_sources,
            navigate_to_sources,
            events: resource_events
                .iter()
                .map(|(node_index, kind)| NativeContentEvent {
                    node_index: *node_index,
                    kind: *kind,
                })
                .collect(),
            csp_violations: loader.take_csp_violations(),
            scroll_commands: Vec::new(),
            navigation: None,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            page_message_port_commands: Vec::new(),
            shared_worker_commands: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            service_worker_client_messages: Vec::new(),
            service_worker_open_windows: Vec::new(),
            window_name: String::new(),
        },
        document,
        viewport,
        script_sources,
        resource_events,
    )))
}

pub(super) fn service_worker_fetch_resource(
    response: NativeFetchResponse,
    fallback_url: &str,
) -> Result<NativeResource, NativeEngineError> {
    let url = if response.url.is_empty() {
        without_fragment(fallback_url).to_owned()
    } else {
        response.url
    };
    let parsed = Url::parse(without_fragment(&url)).map_err(|_| NativeEngineError::Worker {
        operation: "service worker fetch resume".into(),
        reason: "service worker returned an invalid response URL".into(),
    })?;
    Ok(NativeResource {
        url,
        origin: NativeOrigin::from_url(&parsed)?,
        body: String::from_utf8_lossy(&response.body).into_owned(),
    })
}

pub(crate) fn inline_style_policy_nodes(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<BTreeSet<u32>, NativeEngineError> {
    let mut allowed = BTreeSet::new();
    let mut element_nodes = BTreeSet::new();
    let mut attribute_nodes = BTreeSet::new();
    for (node_index, source, nonce) in document.inline_style_elements() {
        element_nodes.insert(node_index);
        let reported =
            document.inline_style_element_reported(node_index, &source, nonce.as_deref());
        let allowed_by_policy = if reported {
            loader.allows_inline_style_element_silent(document_url, &source, nonce.as_deref())?
        } else {
            loader.allows_inline_style_element(document_url, &source, nonce.as_deref())?
        };
        if !reported {
            document.mark_inline_style_element_reported(node_index, source.clone(), nonce.clone());
        }
        if allowed_by_policy {
            allowed.insert(node_index);
        }
    }
    for (node_index, source) in document.inline_style_attributes() {
        attribute_nodes.insert(node_index);
        let reported = document.inline_style_attribute_reported(node_index, &source);
        let allowed_by_policy = if reported {
            loader.allows_inline_style_attribute_silent(document_url, &source)?
        } else {
            loader.allows_inline_style_attribute(document_url, &source)?
        };
        if !reported {
            document.mark_inline_style_attribute_reported(node_index, source.clone());
        }
        if allowed_by_policy {
            allowed.insert(node_index);
        }
    }
    document.prune_inline_style_reports(&element_nodes, &attribute_nodes);
    Ok(allowed)
}

pub(crate) fn refresh_inline_style_policy(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(), NativeEngineError> {
    let allowed = inline_style_policy_nodes(document, loader, document_url)?;
    document.set_inline_style_policy(&allowed);
    Ok(())
}

/// Apply CSP meta policies which were inserted into the live document after
/// parser processing. The document ledger makes this append-only: removing a
/// processed meta element or editing its content cannot relax the policy.
pub(crate) fn apply_pending_meta_content_security_policies(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(), NativeEngineError> {
    let pending = document.unprocessed_content_security_policy_meta();
    if pending.is_empty() {
        return Ok(());
    }
    let node_indexes = pending
        .iter()
        .map(|(node_index, _)| *node_index)
        .collect::<Vec<_>>();
    let policies = pending
        .into_iter()
        .map(|(_, policy)| policy)
        .collect::<Vec<_>>();
    loader.append_meta_content_security_policies(document_url, &policies)?;
    document.mark_content_security_policy_meta_nodes_processed(node_indexes);
    Ok(())
}

/// Drain report-only CSP records produced during a content-process mutation
/// and deliver them through the owning page realm before the turn commits.
/// Listener commands remain part of the same bounded script result so a CSP
/// observer can perform ordinary DOM or network work without losing ordering.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_pending_csp_violations(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    loader: &mut NativeResourceLoader,
    result: &mut NativePageScriptResult,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let violations = loader.take_csp_violations();
    if violations.is_empty() {
        return Ok(());
    }
    let page_events = NativePageEventBatch {
        csp_violations: violations,
        ..NativePageEventBatch::default()
    };
    let evaluation = runtime.evaluate_with_page_events(
        "undefined;",
        document,
        document_url,
        document_origin,
        viewport,
        &page_events,
    )?;
    let commands = evaluation.commands.clone();
    apply_content_event_history(&commands, document_url, document_origin, runtime, history)?;
    let emitted = apply_page_script_evaluation(
        document,
        runtime,
        NativePageScriptContext {
            document_url,
            origin: document_origin,
            viewport,
        },
        evaluation,
        NativePageScriptCommandQueues {
            pending_fetches: &mut result.pending_fetches,
            websocket_commands: &mut result.websocket_commands,
            event_source_commands: &mut result.event_source_commands,
        },
        scroll_commands,
        &mut result.navigation,
    )?;
    events.extend(emitted.into_iter().map(|(node_index, kind)| {
        (
            NativeNodeId::from_parts(document.generation(), node_index),
            kind,
        )
    }));
    result
        .service_worker_commands
        .extend(runtime.take_service_worker_commands());
    result.dialogs.extend(runtime.take_dialog_events());
    Ok(())
}

pub(super) async fn load_external_images(
    document: &mut NativeDocument,
    runtime: Option<&NativeJavaScriptRuntime>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    trace_request_id: Option<u64>,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut image_events = Vec::new();
    for (ordinal, (node_index, source)) in document
        .external_image_links(viewport)
        .into_iter()
        .take(MAX_CONTENT_IMAGES)
        .enumerate()
    {
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.image_load_attempted_for_node(node_id, viewport) {
            continue;
        }
        document.mark_image_load(node_index, source.clone(), viewport)?;
        let preloaded_image = parent_image_preload_cache_key(document_url, &source)
            .and_then(|key| document.take_preloaded_image_resource(&key));
        let object_url = runtime
            .map(|runtime| runtime.object_url_resource(&source))
            .transpose()?
            .flatten();
        let referrer_policy = document.image_referrer_policy_for_node(node_id);
        let resource_stage = trace_request_id
            .map(|request_id| ContentLoadStage::begin_resource(request_id, "image", ordinal));
        let image = if let Some(image) = preloaded_image {
            Ok(Some(image))
        } else if is_parent_owned_network_target(document_url, &source) {
            if let Some(broker) = parent_fetch_broker.as_deref_mut() {
                broker.page_meta_content_security_policies =
                    loader.document_meta_content_security_policies(document_url)?;
                broker
                    .load_image(document_url, &source, referrer_policy)
                    .await
            } else {
                Err(missing_parent_network_authority("image load"))
            }
        } else {
            loader
                .load_image_async_with_object_url_and_referrer_policy(
                    document_url,
                    &source,
                    object_url.as_ref(),
                    referrer_policy,
                )
                .await
        };
        if let Some(resource_stage) = resource_stage {
            resource_stage.finish(None);
        }
        let event_kind = match image {
            Ok(Some(image)) => {
                document.set_image_resource(node_index, source, image)?;
                NativeEventKind::Load
            }
            Ok(None) | Err(_) => NativeEventKind::Error,
        };
        image_events.push((node_index, event_kind));
    }
    for (ordinal, (node_index, source)) in document
        .external_background_image_links()
        .into_iter()
        .take(MAX_CONTENT_IMAGES)
        .enumerate()
    {
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document
            .background_image_resource_for_node(node_id)
            .is_some()
        {
            continue;
        }
        let object_url = runtime
            .map(|runtime| runtime.object_url_resource(&source))
            .transpose()?
            .flatten();
        let resource_stage = trace_request_id.map(|request_id| {
            ContentLoadStage::begin_resource(request_id, "background_image", ordinal)
        });
        let image = if is_parent_owned_network_target(document_url, &source) {
            if let Some(broker) = parent_fetch_broker.as_deref_mut() {
                broker.page_meta_content_security_policies =
                    loader.document_meta_content_security_policies(document_url)?;
                broker
                    .load_image(
                        document_url,
                        &source,
                        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
                    )
                    .await
            } else {
                Err(missing_parent_network_authority("background image load"))
            }
        } else {
            match runtime {
                None => loader.load_image_async(document_url, &source).await,
                Some(_) => {
                    loader
                        .load_image_async_with_object_url(
                            document_url,
                            &source,
                            object_url.as_ref(),
                        )
                        .await
                }
            }
        };
        if let Some(resource_stage) = resource_stage {
            resource_stage.finish(None);
        }
        if let Ok(Some(image)) = image {
            document.set_background_image_resource(node_index, source, image)?;
        }
    }
    Ok(image_events)
}

pub(super) async fn load_external_video_posters(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    trace_request_id: Option<u64>,
) -> Result<(), NativeEngineError> {
    let poster_base_url = Url::parse(&document.effective_base_url(document_url)).ok();
    for (ordinal, (node_index, source)) in document
        .external_video_poster_links()
        .into_iter()
        .take(MAX_CONTENT_IMAGES)
        .enumerate()
    {
        let Some(target_url) = poster_base_url
            .as_ref()
            .and_then(|base_url| base_url.join(&source).ok())
        else {
            continue;
        };
        let target_url = target_url.to_string();
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.video_poster_load_attempted_for_node(node_id) {
            continue;
        }
        if !document.mark_video_poster_load(node_index, source.clone())? {
            continue;
        }
        if !is_parent_owned_network_target(document_url, &target_url) {
            continue;
        }
        let referrer_policy = document.document_referrer_policy();
        let resource_stage = trace_request_id.map(|request_id| {
            ContentLoadStage::begin_resource(request_id, "video_poster", ordinal)
        });
        let image = if let Some(broker) = parent_fetch_broker.as_deref_mut() {
            broker.page_meta_content_security_policies =
                loader.document_meta_content_security_policies(document_url)?;
            broker
                .load_video_poster(document_url, &target_url, referrer_policy)
                .await
        } else {
            Err(missing_parent_network_authority("video poster load"))
        };
        if let Some(resource_stage) = resource_stage {
            resource_stage.finish(None);
        }
        match image {
            Ok(Some(image)) => document.set_video_poster_resource(node_index, source, image)?,
            Ok(None)
            | Err(NativeEngineError::Network { .. })
            | Err(NativeEngineError::UnsupportedUrl { .. })
            | Err(NativeEngineError::LimitExceeded { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(super) fn parent_image_preload_cache_key(document_url: &str, href: &str) -> Option<String> {
    let document_url = Url::parse(without_fragment(document_url)).ok()?;
    let mut image_url = resolve_subresource_url(&document_url, href).ok()??;
    image_url.set_fragment(None);
    Some(image_url.to_string())
}

pub(super) async fn load_external_image_preloads(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    trace_request_id: Option<u64>,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let links = document
        .external_image_preload_links()
        .into_iter()
        .take(MAX_CONTENT_IMAGES)
        .collect::<Vec<_>>();
    let live_nodes = links
        .iter()
        .map(|(node_index, _)| *node_index)
        .collect::<BTreeSet<_>>();
    let mut states = document
        .external_image_preload_states()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    states.retain(|node_index, _| live_nodes.contains(node_index));
    let mut events = Vec::new();

    for (ordinal, (node_index, href)) in links.into_iter().enumerate() {
        if states.get(&node_index) == Some(&href) {
            continue;
        }
        states.insert(node_index, href.clone());
        if !is_parent_owned_network_target(document_url, &href) {
            continue;
        }
        let cache_key = parent_image_preload_cache_key(document_url, &href);
        let cached = cache_key
            .as_deref()
            .is_some_and(|key| document.has_preloaded_image_resource(key));
        let resource_stage = trace_request_id.map(|request_id| {
            ContentLoadStage::begin_resource(request_id, "image_preload", ordinal)
        });
        let event_kind = if cached {
            if let Some(resource_stage) = resource_stage {
                resource_stage.finish_as("cache_hit", None);
            }
            NativeEventKind::Load
        } else {
            let referrer_policy =
                document.stylesheet_link_referrer_policy_for_node_index(node_index);
            let image = if let Some(broker) = parent_fetch_broker.as_deref_mut() {
                broker.page_meta_content_security_policies =
                    loader.document_meta_content_security_policies(document_url)?;
                broker
                    .load_image(document_url, &href, referrer_policy)
                    .await
            } else {
                Err(missing_parent_network_authority("image preload"))
            };
            let event_kind = match image {
                Ok(Some(image)) => {
                    if let Some(cache_key) = cache_key {
                        let _retained =
                            document.cache_preloaded_image_resource(cache_key, image)?;
                    }
                    NativeEventKind::Load
                }
                Ok(None) | Err(_) => NativeEventKind::Error,
            };
            if let Some(resource_stage) = resource_stage {
                resource_stage.finish(None);
            }
            event_kind
        };
        events.push((node_index, event_kind));
    }
    document.set_external_image_preload_states(states);
    Ok(events)
}

pub(super) async fn load_external_media(
    document: &mut NativeDocument,
    runtime: Option<&NativeJavaScriptRuntime>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    trace_request_id: Option<u64>,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut media_events = Vec::new();
    for (ordinal, (node_index, source)) in document
        .external_media_links()
        .into_iter()
        .take(MAX_CONTENT_MEDIA)
        .enumerate()
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.media_load_attempted_for_node(node_id) {
            continue;
        }
        document.mark_media_load(node_index, source.clone())?;
        let resource_stage = trace_request_id
            .map(|request_id| ContentLoadStage::begin_resource(request_id, "media", ordinal));
        let metadata = if !is_blob && is_parent_owned_network_target(document_url, &source) {
            if let Some(broker) = parent_fetch_broker.as_deref_mut() {
                refresh_parent_broker_meta_csp(loader, Some(broker))?;
                broker.load_media(document_url, &source).await?
            } else {
                return Err(missing_parent_network_authority("media load"));
            }
        } else {
            match (is_blob, runtime) {
                (true, Some(runtime)) => {
                    let object_url = runtime.object_url_resource(&source)?;
                    loader.load_local_blob_media(document_url, &source, object_url.as_ref())?
                }
                _ => loader.load_media_async(document_url, &source).await?,
            }
        };
        if let Some(resource_stage) = resource_stage {
            resource_stage.finish(None);
        }
        let event_kind = match metadata {
            Some(metadata) => {
                document.set_media_resource(node_index, source, metadata)?;
                NativeEventKind::Load
            }
            None => {
                document.set_media_error(node_index, source)?;
                NativeEventKind::Error
            }
        };
        media_events.push((node_index, event_kind));
    }
    document.refresh_media_loads();
    Ok(media_events)
}

pub(super) fn expand_network_stylesheet_imports<'a, 'b>(
    loader: &'a mut NativeResourceLoader,
    document_url: &'a str,
    stylesheet: NativeStylesheetResource,
    viewport: Viewport,
    loaded_bytes: &'a mut usize,
    mut parent_fetch_broker: Option<&'a mut NativeContentFetchBroker<'b>>,
) -> Pin<Box<dyn Future<Output = Result<NativeStylesheetResource, NativeEngineError>> + 'a>> {
    Box::pin(async move {
        let mut graph_entries = 0usize;
        let mut graph_bytes = 0usize;
        let mut active = BTreeSet::new();
        let mut loaded = BTreeSet::from([without_fragment(&stylesheet.url).to_owned()]);
        expand_network_stylesheet_body(
            loader,
            document_url,
            stylesheet,
            &mut graph_entries,
            &mut graph_bytes,
            &mut active,
            &mut loaded,
            viewport,
            loaded_bytes,
            parent_fetch_broker.as_deref_mut(),
        )
        .await
    })
}

pub(super) fn expand_network_stylesheet_body<'a, 'b>(
    loader: &'a mut NativeResourceLoader,
    document_url: &'a str,
    stylesheet: NativeStylesheetResource,
    graph_entries: &'a mut usize,
    graph_bytes: &'a mut usize,
    active: &'a mut BTreeSet<String>,
    loaded: &'a mut BTreeSet<String>,
    viewport: Viewport,
    loaded_bytes: &'a mut usize,
    mut parent_fetch_broker: Option<&'a mut NativeContentFetchBroker<'b>>,
) -> Pin<Box<dyn Future<Output = Result<NativeStylesheetResource, NativeEngineError>> + 'a>> {
    Box::pin(async move {
        *graph_entries = graph_entries.saturating_add(1);
        if *graph_entries > MAX_CONTENT_STYLESHEETS {
            return Err(NativeEngineError::limit(
                "network stylesheet graph entries",
                MAX_CONTENT_STYLESHEETS,
                *graph_entries,
            ));
        }
        *graph_bytes = graph_bytes.saturating_add(stylesheet.body.len());
        if *graph_bytes > MAX_CONTENT_STYLESHEET_BYTES {
            return Err(NativeEngineError::limit(
                "network stylesheet graph bytes",
                MAX_CONTENT_STYLESHEET_BYTES,
                *graph_bytes,
            ));
        }
        let next_loaded_bytes = loaded_bytes.saturating_add(stylesheet.body.len());
        if next_loaded_bytes > MAX_CONTENT_STYLESHEET_BYTES {
            return Err(NativeEngineError::limit(
                "network stylesheet bytes",
                MAX_CONTENT_STYLESHEET_BYTES,
                next_loaded_bytes,
            ));
        }
        *loaded_bytes = next_loaded_bytes;

        let stylesheet_url = without_fragment(&stylesheet.url).to_owned();
        active.insert(stylesheet_url.clone());
        let imports = static_css_imports(&stylesheet.body).map_err(|reason| {
            NativeEngineError::UnsupportedUrl {
                reason: format!("network stylesheet import: {reason}"),
            }
        })?;
        let mut expanded = String::with_capacity(stylesheet.body.len());
        let mut cursor = 0usize;
        for import in imports {
            expanded.push_str(&stylesheet.body[cursor..import.start]);
            if !css_import_matches(&import, viewport) {
                cursor = import.end;
                continue;
            }
            let specifier = decode_css_url_value(&import.specifier).ok_or_else(|| {
                NativeEngineError::UnsupportedUrl {
                    reason: "network stylesheet import contains an invalid CSS escape".into(),
                }
            })?;
            let base_url =
                Url::parse(&stylesheet_url).map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "network stylesheet owner URL is not valid URL syntax".into(),
                })?;
            let target = resolve_subresource_url(&base_url, &specifier)?.ok_or_else(|| {
                NativeEngineError::UnsupportedUrl {
                    reason: format!(
                        "network stylesheet dependency {:?} is not an HTTP(S) URL",
                        import.specifier
                    ),
                }
            })?;
            let target = without_fragment(target.as_str()).to_owned();
            if active.contains(&target) || !loaded.insert(target.clone()) {
                cursor = import.end;
                continue;
            }
            let dependency_result = if is_parent_owned_network_target(document_url, &target) {
                if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
                    parent_fetch_broker
                        .load_stylesheet(document_url, &target, None, None, None)
                        .await
                } else {
                    Err(missing_parent_network_authority(
                        "network stylesheet dependency",
                    ))
                }
            } else {
                loader
                    .load_stylesheet_async(document_url, &target, None, None)
                    .await
            };
            let dependency = dependency_result?.ok_or_else(|| NativeEngineError::Network {
                operation: "network stylesheet dependency".into(),
                reason: format!(
                    "network stylesheet dependency {:?} was blocked or unavailable",
                    import.specifier
                ),
            })?;
            let dependency_url = without_fragment(&dependency.url).to_owned();
            if dependency_url != target && !loaded.insert(dependency_url.clone()) {
                cursor = import.end;
                continue;
            }
            let dependency = expand_network_stylesheet_body(
                loader,
                document_url,
                dependency,
                graph_entries,
                graph_bytes,
                active,
                loaded,
                viewport,
                loaded_bytes,
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
            if let Some(layer) = import.layer {
                expanded.push_str("@layer ");
                expanded.push_str(&layer);
                expanded.push_str(" { ");
                expanded.push_str(&dependency.body);
                expanded.push_str(" }");
            } else {
                expanded.push_str(&dependency.body);
            }
            cursor = import.end;
        }
        expanded.push_str(&stylesheet.body[cursor..]);
        active.remove(&stylesheet_url);
        Ok(NativeStylesheetResource {
            url: stylesheet_url.clone(),
            body: absolutize_stylesheet_urls(&expanded, document_url, &stylesheet_url),
        })
    })
}

pub(super) async fn load_dynamic_external_stylesheets(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let links = document
        .external_stylesheet_links()
        .into_iter()
        .take(MAX_CONTENT_STYLESHEETS)
        .collect::<Vec<_>>();
    let previous_states = document.external_stylesheet_states();
    let live_nodes = links
        .iter()
        .map(|(node_index, _, _, _)| *node_index)
        .collect::<BTreeSet<_>>();
    let mut states = document
        .external_stylesheet_states()
        .into_iter()
        .map(|(node_index, href, stylesheet_url, body)| (node_index, (href, stylesheet_url, body)))
        .collect::<BTreeMap<_, _>>();
    states.retain(|node_index, _| live_nodes.contains(node_index));
    let mut loaded_bytes = states
        .values()
        .filter_map(|(_, _, body)| body.as_ref())
        .map(String::len)
        .sum::<usize>();
    let mut events = Vec::new();

    for (node_index, href, integrity, crossorigin) in links {
        if states
            .get(&node_index)
            .is_some_and(|(loaded_href, _, _)| loaded_href == &href)
        {
            continue;
        }
        if let Some((_, _, previous_body)) = states.get(&node_index)
            && let Some(previous_body) = previous_body
        {
            loaded_bytes = loaded_bytes.saturating_sub(previous_body.len());
        }
        let object_url = runtime.object_url_resource(&href)?;
        let referrer_policy = document.stylesheet_link_referrer_policy_for_node_index(node_index);
        let stylesheet_result =
            if object_url.is_none() && is_parent_owned_network_target(document_url, &href) {
                if let Some(broker) = parent_fetch_broker.as_deref_mut() {
                    refresh_parent_broker_meta_csp(loader, Some(broker))?;
                    broker
                        .load_stylesheet(
                            document_url,
                            &href,
                            integrity.as_deref(),
                            crossorigin.as_deref(),
                            Some(referrer_policy),
                        )
                        .await
                } else {
                    Err(missing_parent_network_authority("dynamic stylesheet load"))
                }
            } else {
                loader
                    .load_stylesheet_async_with_object_url_and_referrer_policy(
                        document_url,
                        &href,
                        integrity.as_deref(),
                        crossorigin.as_deref(),
                        object_url.as_ref(),
                        Some(referrer_policy),
                    )
                    .await
            };
        let body = match stylesheet_result {
            Ok(Some(stylesheet)) => {
                if is_network_url(&stylesheet.url) {
                    expand_network_stylesheet_imports(
                        loader,
                        document_url,
                        stylesheet,
                        viewport,
                        &mut loaded_bytes,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await
                    .ok()
                } else {
                    let next_len = loaded_bytes.saturating_add(stylesheet.body.len());
                    if next_len <= MAX_CONTENT_STYLESHEET_BYTES {
                        loaded_bytes = next_len;
                        Some(stylesheet)
                    } else {
                        None
                    }
                }
            }
            Ok(None) | Err(_) => None,
        };
        let event_kind = body
            .as_ref()
            .map_or(NativeEventKind::Error, |_| NativeEventKind::Load);
        let stylesheet_url = body.as_ref().map(|stylesheet| stylesheet.url.clone());
        let body = body.map(|stylesheet| stylesheet.body);
        states.insert(node_index, (href, stylesheet_url, body));
        events.push((node_index, event_kind));
    }

    let next_states = states
        .into_iter()
        .map(|(node_index, (href, stylesheet_url, body))| (node_index, href, stylesheet_url, body))
        .collect::<Vec<_>>();
    if previous_states != next_states {
        document.set_external_stylesheet_states(next_states);
        document.rebuild_external_stylesheet(document_url)?;
        load_font_faces(
            document,
            Some(runtime),
            loader,
            document_url,
            parent_fetch_broker,
        )
        .await?;
        document.refresh_background_image_sources();
    }
    Ok(events)
}

pub(super) async fn load_page_script_sources(
    document: &NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    trace_request_id: Option<u64>,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    load_page_script_source_list(
        document.page_script_sources(
            crate::native_engine::javascript::MAX_NATIVE_INLINE_SCRIPTS,
            MAX_NATIVE_SCRIPT_BYTES,
        ),
        document.page_import_map_sources(
            crate::native_engine::javascript::MAX_NATIVE_INLINE_SCRIPTS,
            MAX_NATIVE_SCRIPT_BYTES,
        ),
        loader,
        document_url,
        "glass-inline-module",
        None,
        parent_fetch_broker,
        trace_request_id,
    )
    .await
}

pub(super) async fn load_dynamic_page_script_sources(
    sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    runtime: &NativeJavaScriptRuntime,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    load_page_script_source_list(
        sources,
        Vec::new(),
        loader,
        document_url,
        "glass-dynamic-module",
        Some(runtime),
        parent_fetch_broker,
        None,
    )
    .await
}

pub(crate) fn is_network_page_script_target(document_url: &str, href: &str) -> bool {
    let Ok(base_url) = Url::parse(document_url) else {
        return false;
    };
    let target = Url::parse(href).or_else(|_| base_url.join(href));
    target.is_ok_and(|target| is_network_url(target.as_str()))
}

pub(crate) fn is_parent_owned_network_target(document_url: &str, href: &str) -> bool {
    // Cookie authority follows the HTTP(S) request target, not the scheme of
    // the document that initiated it. A file:, data:, or fixture: document
    // must not create a transient child cookie jar for remote requests.
    is_network_page_script_target(document_url, href)
}

pub(super) async fn load_page_script_source_list(
    page_sources: Vec<NativePageScriptSource>,
    page_import_maps: Vec<NativePageImportMapSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    module_name_prefix: &str,
    runtime: Option<&NativeJavaScriptRuntime>,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    trace_request_id: Option<u64>,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    let mut sources = Vec::new();
    let mut resource_events = Vec::new();

    let mut import_map = runtime
        .map(NativeJavaScriptRuntime::module_import_map)
        .transpose()?
        .unwrap_or_default();
    sources.push((
        NativePageScriptTiming::ParserBlocking,
        NativePageScript::ImportMap(import_map.clone()),
    ));
    let mut page_import_maps = page_import_maps.into_iter().peekable();
    for (index, script) in page_sources.into_iter().enumerate() {
        let node_index = script.node_index();
        while page_import_maps
            .peek()
            .is_some_and(|map_source| map_source.node_index < node_index)
        {
            let map_source = page_import_maps
                .next()
                .expect("peeked import-map source remains available");
            register_page_import_map_source(
                map_source,
                &mut import_map,
                loader,
                document_url,
                &mut sources,
                &mut resource_events,
            )?;
        }
        match script {
            NativePageScriptSource::ImportMap {
                source,
                node_index,
                nonce,
            } => register_page_import_map_source(
                NativePageImportMapSource {
                    source,
                    node_index,
                    nonce,
                },
                &mut import_map,
                loader,
                document_url,
                &mut sources,
                &mut resource_events,
            )?,
            NativePageScriptSource::Inline {
                source,
                timing,
                node_index,
                nonce,
                parser_inserted: _,
            } => {
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                sources.push((
                    timing,
                    NativePageScript::Classic {
                        source,
                        base_url: document_url.to_owned(),
                        node_index: Some(node_index),
                    },
                ));
            }
            NativePageScriptSource::ModuleInline {
                source,
                timing,
                node_index,
                nonce,
                referrer_policy,
                parser_inserted: _,
            } => {
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                let name = format!("{document_url}#{module_name_prefix}-{index}");
                let mut seen = BTreeSet::new();
                seen.insert(name.clone());
                let mut total_bytes = source.len();
                let script_start = sources.len();
                sources.push((
                    timing,
                    NativePageScript::Module {
                        name: name.clone(),
                        source: source.clone(),
                        base_url: document_url.to_owned(),
                        referrer_policy: Some(referrer_policy),
                        node_index: Some(node_index),
                    },
                ));
                let dependency_result = load_module_dependencies(
                    document_url,
                    &name,
                    document_url,
                    &source,
                    Some(referrer_policy),
                    timing,
                    loader,
                    runtime,
                    &mut import_map,
                    &mut sources,
                    &mut seen,
                    &mut total_bytes,
                    parent_fetch_broker.as_deref_mut(),
                )
                .await;
                if dependency_result.is_err() {
                    sources.truncate(script_start);
                }
                sources.push((
                    NativePageScriptTiming::ParserBlocking,
                    NativePageScript::ImportMap(import_map.clone()),
                ));
            }
            NativePageScriptSource::External {
                href,
                timing,
                node_index,
                nonce,
                integrity,
                crossorigin,
                referrer_policy,
                parser_inserted,
            } => {
                let resource_stage = trace_request_id.map(|request_id| {
                    ContentLoadStage::begin_resource(request_id, "external_script", index)
                });
                let object_url = runtime
                    .map(|runtime| runtime.object_url_resource(&href))
                    .transpose()?
                    .flatten();
                let load_result = if object_url.is_none()
                    && is_parent_owned_network_target(document_url, &href)
                {
                    if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
                        parent_fetch_broker
                            .load_page_script(
                                document_url,
                                &href,
                                None,
                                MAX_NATIVE_SCRIPT_BYTES,
                                parser_inserted,
                                nonce.as_deref(),
                                integrity.as_deref(),
                                crossorigin.as_deref(),
                                None,
                                Some(referrer_policy),
                            )
                            .await
                    } else {
                        Err(missing_parent_network_authority("classic script load"))
                    }
                } else {
                    loader
                        .load_script_async_with_metadata_and_object_url(NativeScriptLoadRequest {
                            document_url,
                            referrer_url: None,
                            href: &href,
                            max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                            subresource_kind: NativeSubresourceKind::Script,
                            parser_inserted,
                            nonce: nonce.as_deref(),
                            integrity: integrity.as_deref(),
                            crossorigin: crossorigin.as_deref(),
                            object_url: object_url.as_ref(),
                            module_type: None,
                            worker_credentials_mode: None,
                            referrer_policy: Some(referrer_policy),
                        })
                        .await
                };
                if let Some(resource_stage) = resource_stage {
                    resource_stage.finish(None);
                }
                match load_result {
                    Ok(resource) => match resource {
                        Some(resource) => {
                            resource_events.push((node_index, NativeEventKind::Load));
                            sources.push((
                                timing,
                                NativePageScript::Classic {
                                    source: resource.body,
                                    base_url: resource.url,
                                    node_index: Some(node_index),
                                },
                            ));
                        }
                        None => resource_events.push((node_index, NativeEventKind::Error)),
                    },
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
            NativePageScriptSource::ModuleExternal {
                href,
                timing,
                node_index,
                nonce,
                integrity,
                crossorigin,
                referrer_policy,
                parser_inserted,
            } => {
                let request_url = match resolve_module_request_url(document_url, &href) {
                    Ok(request_url) => request_url,
                    Err(_) => {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    }
                };
                let resource_stage = trace_request_id.map(|request_id| {
                    ContentLoadStage::begin_resource(request_id, "external_module", index)
                });
                let object_url = runtime
                    .map(|runtime| runtime.object_url_resource(&href))
                    .transpose()?
                    .flatten();
                let crossorigin = crossorigin.as_deref().or(Some("anonymous"));
                let load_result = if object_url.is_none()
                    && is_parent_owned_network_target(document_url, &href)
                {
                    if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
                        parent_fetch_broker
                            .load_page_script(
                                document_url,
                                &href,
                                None,
                                MAX_NATIVE_SCRIPT_BYTES,
                                parser_inserted,
                                nonce.as_deref(),
                                integrity.as_deref(),
                                crossorigin,
                                None,
                                Some(referrer_policy),
                            )
                            .await
                    } else {
                        Err(missing_parent_network_authority("module script load"))
                    }
                } else {
                    loader
                        .load_script_async_with_metadata_and_object_url(NativeScriptLoadRequest {
                            document_url,
                            referrer_url: None,
                            href: &href,
                            max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                            subresource_kind: NativeSubresourceKind::Script,
                            parser_inserted,
                            nonce: nonce.as_deref(),
                            integrity: integrity.as_deref(),
                            crossorigin,
                            object_url: object_url.as_ref(),
                            module_type: None,
                            worker_credentials_mode: None,
                            referrer_policy: Some(referrer_policy),
                        })
                        .await
                };
                if let Some(resource_stage) = resource_stage {
                    resource_stage.finish(None);
                }
                match load_result {
                    Ok(resource) => match resource {
                        Some(resource) => {
                            let script_start = sources.len();
                            let name = request_url;
                            let base_url = resource.url;
                            let source = resource.body;
                            let dependency_referrer_policy =
                                resource.response_referrer_policy.unwrap_or(referrer_policy);
                            let mut seen = BTreeSet::new();
                            seen.insert(name.clone());
                            let mut total_bytes = source.len();
                            sources.push((
                                timing,
                                NativePageScript::Module {
                                    name: name.clone(),
                                    source: source.clone(),
                                    base_url: base_url.clone(),
                                    referrer_policy: Some(dependency_referrer_policy),
                                    node_index: Some(node_index),
                                },
                            ));
                            let dependency_result = load_module_dependencies(
                                document_url,
                                &name,
                                &base_url,
                                &source,
                                Some(dependency_referrer_policy),
                                timing,
                                loader,
                                runtime,
                                &mut import_map,
                                &mut sources,
                                &mut seen,
                                &mut total_bytes,
                                parent_fetch_broker.as_deref_mut(),
                            )
                            .await;
                            if dependency_result.is_ok() {
                                resource_events.push((node_index, NativeEventKind::Load));
                            } else {
                                sources.truncate(script_start);
                                resource_events.push((node_index, NativeEventKind::Error));
                            }
                            sources.push((
                                NativePageScriptTiming::ParserBlocking,
                                NativePageScript::ImportMap(import_map.clone()),
                            ));
                        }
                        None => resource_events.push((node_index, NativeEventKind::Error)),
                    },
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
        }
    }
    for map_source in page_import_maps {
        register_page_import_map_source(
            map_source,
            &mut import_map,
            loader,
            document_url,
            &mut sources,
            &mut resource_events,
        )?;
    }
    Ok((order_page_scripts(sources), resource_events))
}
