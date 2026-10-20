//! Service worker fetch, cache, messaging, and client operation handling.

use super::*;

pub(super) fn validate_service_worker_message_port_bridge_key(
    bridge_key: &str,
) -> Result<(), NativeEngineError> {
    validate_url_text("native service worker MessagePort bridge key", bridge_key)?;
    if bridge_key.is_empty() {
        return Err(NativeEngineError::invalid(
            "native service worker MessagePort bridge key",
            "must not be empty",
        ));
    }
    if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker MessagePort bridge key",
            crate::limits::MAX_BACKEND_ID_BYTES,
            bridge_key.len(),
        ));
    }
    Ok(())
}

pub(super) fn service_worker_scope_matches(scope_path: &str, target_path: &str) -> bool {
    target_path == scope_path
        || (scope_path.ends_with('/') && target_path.starts_with(scope_path))
        || target_path
            .strip_prefix(scope_path)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

pub(super) async fn fetch_service_worker_navigation_preload<'broker>(
    loader: &mut NativeResourceLoader,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    request_id: u32,
    target_url: &str,
    referrer: Option<&str>,
    referrer_policy: NativeFetchReferrerPolicy,
    header_value: &str,
) -> Result<NativeFetchResponse, NativeEngineError> {
    validate_navigation_preload_header_value(header_value)?;
    let referrer = referrer.filter(|value| !value.is_empty());
    let owner_url = referrer.unwrap_or(target_url);
    let referrer_url = referrer.map(str::to_owned).unwrap_or_default();
    if let Some(parent_fetch_broker) = parent_fetch_broker {
        let request = NativeFetchRequest {
            document_url: owner_url,
            href: target_url,
            referrer_url: Some(referrer_url),
            referrer_policy: Some(referrer_policy.as_str().to_owned()),
            method: NativeFetchMethod::get(),
            body: None,
            content_type: None,
            request_headers: BTreeMap::from([
                (
                    "accept".to_owned(),
                    "text/html,application/xhtml+xml".to_owned(),
                ),
                (
                    "service-worker-navigation-preload".to_owned(),
                    header_value.to_owned(),
                ),
            ]),
            credentials: true,
            credentials_mode: Some(NativeFetchCredentialsMode::Include),
            cors_mode: NativeCorsMode::Navigation,
            redirect_mode: NativeFetchRedirectMode::Follow,
            cache_mode: NativeFetchCacheMode::Default,
            timeout: None,
            max_response_bytes: Some(loader.max_document_bytes().min(MAX_NATIVE_FORM_BODY_BYTES)),
        };
        parent_fetch_broker
            .fetch_for_captured_navigation(request_id, &request)
            .await?
            .0
    } else {
        Err(missing_parent_network_authority(
            "Service Worker navigation preload",
        ))
    }
}

pub(super) async fn settle_service_worker_cache_event(
    worker: &mut NativeServiceWorker,
    evaluation: NativeScriptEvaluation,
    loader: &mut NativeResourceLoader,
    cache_state: &mut NativeServiceWorkerCacheState,
    _current_client_id: Option<&str>,
    pending_open_windows: &mut VecDeque<NativeServiceWorkerOpenWindowRequest>,
) -> Result<Vec<NativeServiceWorkerClientMessage>, NativeEngineError> {
    settle_service_worker_cache_event_with_parent_fetch_broker(
        worker,
        evaluation,
        loader,
        cache_state,
        _current_client_id,
        pending_open_windows,
        NativeServiceWorkerFetchAuthority {
            parent_fetch_broker: None,
            allow_captured_load_fetch: false,
        },
    )
    .await
}

pub(super) struct NativeServiceWorkerFetchAuthority<'borrow, 'broker> {
    pub(super) parent_fetch_broker: Option<&'borrow mut NativeContentFetchBroker<'broker>>,
    pub(super) allow_captured_load_fetch: bool,
}

pub(super) struct NativeServiceWorkerFetchSettlementContext<'borrow, 'broker> {
    pub(super) current_client_id: &'borrow str,
    pub(super) fallback_url: &'borrow str,
    pub(super) pending_open_windows: &'borrow mut VecDeque<NativeServiceWorkerOpenWindowRequest>,
    pub(super) lifetime_fetch_tasks: &'borrow mut JoinSet<NativeServiceWorkerFetchTaskResult>,
    pub(super) lifetime_fetch_abort_handles: &'borrow mut BTreeMap<(u32, u32), AbortHandle>,
    pub(super) authority: NativeServiceWorkerFetchAuthority<'borrow, 'broker>,
}

#[cfg(test)]
pub(super) struct NativeServiceWorkerFetchTestContext<'a> {
    pub(super) current_client_id: &'a str,
    pub(super) fallback_url: &'a str,
    pub(super) pending_open_windows: &'a mut VecDeque<NativeServiceWorkerOpenWindowRequest>,
    pub(super) lifetime_fetch_tasks: &'a mut JoinSet<NativeServiceWorkerFetchTaskResult>,
    pub(super) lifetime_fetch_abort_handles: &'a mut BTreeMap<(u32, u32), AbortHandle>,
}

pub(super) async fn settle_service_worker_cache_event_with_parent_fetch_broker<'borrow, 'broker>(
    worker: &mut NativeServiceWorker,
    evaluation: NativeScriptEvaluation,
    loader: &mut NativeResourceLoader,
    cache_state: &mut NativeServiceWorkerCacheState,
    _current_client_id: Option<&str>,
    pending_open_windows: &mut VecDeque<NativeServiceWorkerOpenWindowRequest>,
    authority: NativeServiceWorkerFetchAuthority<'borrow, 'broker>,
) -> Result<Vec<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let NativeServiceWorkerFetchAuthority {
        mut parent_fetch_broker,
        allow_captured_load_fetch,
    } = authority;
    let mut pending = VecDeque::from(evaluation.commands);
    let mut value = evaluation.value;
    let mut awaiting = evaluation.top_level_await_pending;
    let mut open_window_pending = false;
    let mut client_messages = Vec::new();
    let mut turns = 0usize;
    if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
        value = resolved_value;
        awaiting = false;
    }
    while let Some(command) = pending.pop_front() {
        turns = turns.saturating_add(1);
        if turns > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "service worker cache event turns",
                MAX_NATIVE_MODULE_IMPORTS,
                turns,
            ));
        }
        if apply_service_worker_lifecycle_command(worker, &command)? {
            continue;
        }
        if let Some(message) = service_worker_client_message_command(worker.id, command.clone())? {
            client_messages.push(message);
            continue;
        }
        if let Some(request) = service_worker_open_window_command(worker, command.clone())? {
            if pending_open_windows.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native service worker openWindow requests",
                    MAX_NATIVE_EFFECTS,
                    pending_open_windows.len().saturating_add(1),
                ));
            }
            pending_open_windows.push_back(request);
            open_window_pending = true;
            continue;
        }
        if resolve_service_worker_fetch_command(
            worker,
            loader,
            command.clone(),
            NativeServiceWorkerFetchCommandState {
                pending: &mut pending,
                awaiting: &mut awaiting,
                value: &mut value,
            },
            parent_fetch_broker.as_deref_mut(),
            allow_captured_load_fetch,
        )
        .await?
        {
            continue;
        }
        let (request_id, payload) =
            apply_service_worker_cache_command(worker, command, cache_state)?;
        let resolved = worker.runtime.resolve_service_worker_cache(
            worker.id,
            &worker.script_url,
            request_id,
            &payload,
            worker.is_module,
        )?;
        pending.extend(resolved.commands);
        awaiting |= resolved.top_level_await_pending;
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            value = resolved_value;
            awaiting = false;
        }
    }
    if awaiting && worker.runtime.take_top_level_await_result()?.is_none() && !open_window_pending {
        return Err(NativeEngineError::Worker {
            operation: "service worker cache event".into(),
            reason: "service worker cache promise remained pending".into(),
        });
    }
    Ok(client_messages)
}

pub(super) fn service_worker_client_message_command(
    worker_id: u32,
    command: NativeScriptCommand,
) -> Result<Option<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let NativeScriptCommand::ServiceWorkerClientPostMessage {
        worker_id: command_worker_id,
        client_id,
        data,
        transfer_ports,
        object_urls,
    } = command
    else {
        return Ok(None);
    };
    if command_worker_id == 0 || command_worker_id != worker_id {
        return Err(NativeEngineError::Worker {
            operation: "service worker client message".into(),
            reason: "service worker client message owner is invalid".into(),
        });
    }
    if client_id.is_empty() {
        return Err(NativeEngineError::invalid(
            "native service worker client id",
            "must not be empty",
        ));
    }
    if client_id.len() > crate::limits::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker client id",
            crate::limits::MAX_BACKEND_ID_BYTES,
            client_id.len(),
        ));
    }
    validate_message_port_transfers(&transfer_ports)?;
    validate_native_object_url_transfers(&object_urls)?;
    validate_native_message_payload(
        &json!({
            "data": &data,
            "transfer_ports": &transfer_ports,
            "object_urls": &object_urls,
        }),
        "native service worker client message",
    )?;
    Ok(Some(NativeServiceWorkerClientMessage {
        worker_id,
        client_id,
        data,
        transfer_ports,
        object_urls,
    }))
}

pub(super) fn service_worker_open_window_command(
    worker: &NativeServiceWorker,
    command: NativeScriptCommand,
) -> Result<Option<NativeServiceWorkerOpenWindowRequest>, NativeEngineError> {
    let NativeScriptCommand::ServiceWorkerOpenWindow {
        request_id,
        worker_id,
        url,
    } = command
    else {
        return Ok(None);
    };
    if request_id == 0 || worker_id == 0 || worker_id != worker.id {
        return Err(NativeEngineError::Worker {
            operation: "service worker openWindow request".into(),
            reason: "service worker openWindow request owner is invalid".into(),
        });
    }
    let target = parse_network_url("service worker openWindow URL", &url)?;
    let worker_url = parse_network_url("service worker URL", &worker.script_url)?;
    if NativeOrigin::from_url(&target)? != NativeOrigin::from_url(&worker_url)? {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "service worker openWindow URL must be same-origin".into(),
        });
    }
    Ok(Some(NativeServiceWorkerOpenWindowRequest {
        request_id,
        worker_id,
        url: without_fragment(target.as_str()).to_owned(),
        source_context_id: String::new(),
        source_frame_id: String::new(),
    }))
}

pub(super) fn cancel_service_worker_fetch_upload(
    worker: &mut NativeServiceWorker,
    request_id: u32,
) {
    if let Some(connection) = worker.fetch_upload_connections.remove(&request_id) {
        let _ = connection
            .commands
            .try_send(NativeFetchUploadCommand::Cancel);
    }
}

pub(super) fn process_service_worker_fetch_upload_command(
    worker: &mut NativeServiceWorker,
    request_id: u32,
    command: NativeScriptCommand,
) -> Result<bool, NativeEngineError> {
    let upload_command = matches!(
        &command,
        NativeScriptCommand::FetchUploadChunk { .. }
            | NativeScriptCommand::FetchUploadEnd { .. }
            | NativeScriptCommand::FetchUploadError { .. }
            | NativeScriptCommand::FetchUploadCancel { .. }
    );
    if !upload_command {
        return Ok(false);
    }
    match command {
        NativeScriptCommand::FetchUploadChunk {
            stream_id,
            data_base64,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            let connection = worker
                .fetch_upload_connections
                .get_mut(&request_id)
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "service worker fetch request upload chunk".into(),
                    reason: "service worker fetch request upload identifier is not active".into(),
                })?;
            if !connection.demand_pending {
                return Err(NativeEngineError::Network {
                    operation: "service worker fetch request upload chunk".into(),
                    reason: "service worker fetch request upload chunk arrived without demand"
                        .into(),
                });
            }
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_base64)
                .map_err(|_| {
                    NativeEngineError::invalid(
                        "service worker fetch request upload chunk",
                        "must be valid base64",
                    )
                })?;
            let next_total = connection.total_bytes.saturating_add(data.len());
            let next_chunks = connection.chunk_count.saturating_add(1);
            if next_total > MAX_NATIVE_FORM_BODY_BYTES
                || next_chunks > MAX_NATIVE_FETCH_UPLOAD_CHUNKS
            {
                let connection = worker
                    .fetch_upload_connections
                    .remove(&request_id)
                    .expect("service worker fetch upload connection was checked above");
                let message = if next_total > MAX_NATIVE_FORM_BODY_BYTES {
                    "native service worker fetch request upload body exceeds its limit"
                } else {
                    "native service worker fetch request upload chunk limit exceeded"
                };
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Error {
                        message: message.into(),
                    });
                return Ok(true);
            }
            connection.total_bytes = next_total;
            connection.chunk_count = next_chunks;
            connection.demand_pending = false;
            connection
                .commands
                .try_send(NativeFetchUploadCommand::Chunk { data })
                .map_err(|_| NativeEngineError::Network {
                    operation: "service worker fetch request upload chunk".into(),
                    reason: "service worker fetch request upload task is unavailable".into(),
                })?;
            Ok(true)
        }
        NativeScriptCommand::FetchUploadEnd {
            stream_id,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            let mut connection = worker
                .fetch_upload_connections
                .remove(&request_id)
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "service worker fetch request upload end".into(),
                    reason: "service worker fetch request upload identifier is not active".into(),
                })?;
            if !connection.demand_pending {
                return Err(NativeEngineError::Network {
                    operation: "service worker fetch request upload end".into(),
                    reason: "service worker fetch request upload ended without demand".into(),
                });
            }
            connection.demand_pending = false;
            connection
                .commands
                .try_send(NativeFetchUploadCommand::End)
                .map_err(|_| NativeEngineError::Network {
                    operation: "service worker fetch request upload end".into(),
                    reason: "service worker fetch request upload task is unavailable".into(),
                })?;
            Ok(true)
        }
        NativeScriptCommand::FetchUploadError {
            stream_id,
            message,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            let connection = worker
                .fetch_upload_connections
                .remove(&request_id)
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "service worker fetch request upload error".into(),
                    reason: "service worker fetch request upload identifier is not active".into(),
                })?;
            let message = message
                .chars()
                .take(crate::limits::MAX_TEXT_BYTES)
                .collect();
            let _ = connection
                .commands
                .try_send(NativeFetchUploadCommand::Error { message });
            Ok(true)
        }
        NativeScriptCommand::FetchUploadCancel {
            stream_id,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            cancel_service_worker_fetch_upload(worker, request_id);
            Ok(true)
        }
        _ => Err(NativeEngineError::invalid(
            "service worker fetch request upload command",
            "worker id or upload stream identifier is invalid",
        )),
    }
}

/// Collect a bounded Service Worker request body for the parent Fetch broker.
/// The worker remains the sole producer and is resumed only when the collector
/// asks for the next chunk; unrelated commands return to the owning turn.
pub(super) async fn collect_service_worker_fetch_upload(
    worker: &mut NativeServiceWorker,
    request_id: u32,
    pending: &mut VecDeque<NativeScriptCommand>,
) -> Result<Result<Vec<u8>, NativeEngineError>, NativeEngineError> {
    if worker.fetch_upload_connections.contains_key(&request_id) {
        return Err(NativeEngineError::Network {
            operation: "service worker fetch request upload".into(),
            reason: "service worker fetch upload identifier is already active".into(),
        });
    }
    if worker.fetch_upload_connections.len() >= MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS {
        return Err(NativeEngineError::limit(
            "native service worker fetch request uploads",
            MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS,
            worker.fetch_upload_connections.len().saturating_add(1),
        ));
    }
    let (upload_connection, upload_source) = spawn_native_fetch_upload_source();
    worker
        .fetch_upload_connections
        .insert(request_id, upload_connection);
    let mut task = Some(tokio::spawn(async move {
        upload_source.collect(MAX_NATIVE_FORM_BODY_BYTES).await
    }));
    loop {
        if task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            let collected = match task
                .take()
                .expect("active Service Worker upload collector task")
                .await
            {
                Ok(collected) => collected,
                Err(_) => {
                    cancel_service_worker_fetch_upload(worker, request_id);
                    return Err(NativeEngineError::Worker {
                        operation: "service worker fetch request upload collector".into(),
                        reason: "native service worker upload collector terminated unexpectedly"
                            .into(),
                    });
                }
            };
            cancel_service_worker_fetch_upload(worker, request_id);
            return Ok(collected.map_err(|reason| NativeEngineError::Network {
                operation: "service worker fetch request upload".into(),
                reason,
            }));
        }

        let mut disconnected = false;
        let demand = if let Some(connection) = worker.fetch_upload_connections.get_mut(&request_id)
        {
            match connection.events.try_recv() {
                Ok(NativeFetchUploadEvent::Demand) => {
                    connection.demand_pending = true;
                    true
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => false,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    disconnected = true;
                    false
                }
            }
        } else {
            false
        };
        if disconnected {
            task.take()
                .expect("active Service Worker upload collector task")
                .abort();
            cancel_service_worker_fetch_upload(worker, request_id);
            return Err(NativeEngineError::Network {
                operation: "service worker fetch request upload".into(),
                reason: "service worker fetch upload collector is unavailable".into(),
            });
        }
        if demand {
            let evaluation = match worker.runtime.evaluate_service_worker_fetch_upload_event(
                worker.id,
                &worker.script_url,
                request_id,
                &json!({"type": "demand"}),
                worker.is_module,
            ) {
                Ok(evaluation) => evaluation,
                Err(error) => {
                    task.take()
                        .expect("active Service Worker upload collector task")
                        .abort();
                    cancel_service_worker_fetch_upload(worker, request_id);
                    return Err(error);
                }
            };
            for command in evaluation.commands {
                let consumed = match process_service_worker_fetch_upload_command(
                    worker,
                    request_id,
                    command.clone(),
                ) {
                    Ok(consumed) => consumed,
                    Err(error) => {
                        task.take()
                            .expect("active Service Worker upload collector task")
                            .abort();
                        cancel_service_worker_fetch_upload(worker, request_id);
                        return Err(error);
                    }
                };
                if !consumed {
                    pending.push_back(command);
                }
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}

/// Drive a Service Worker-owned request body until its HTTP fetch has
/// completed. The Service Worker remains the sole stream producer: each
/// upload demand re-enters its serialized realm and any unrelated commands
/// emitted by that body callback return to the existing command queue.
pub(super) struct NativeServiceWorkerFetchUploadRequest {
    request_id: u32,
    href: String,
    method: NativeFetchMethod,
    headers: BTreeMap<String, String>,
    content_type: Option<String>,
    credentials: bool,
    credentials_mode: NativeFetchCredentialsMode,
    referrer_url: Option<String>,
    referrer_policy: Option<String>,
    cors_mode: NativeCorsMode,
    redirect_mode: NativeFetchRedirectMode,
    cache_mode: NativeFetchCacheMode,
    timeout: Option<Duration>,
}

pub(super) async fn open_service_worker_fetch_upload(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    request: NativeServiceWorkerFetchUploadRequest,
    pending: &mut VecDeque<NativeScriptCommand>,
) -> Result<Result<NativeFetchResponse, NativeEngineError>, NativeEngineError> {
    if worker
        .fetch_upload_connections
        .contains_key(&request.request_id)
    {
        return Err(NativeEngineError::Network {
            operation: "service worker fetch request upload".into(),
            reason: "service worker fetch upload identifier is already active".into(),
        });
    }
    if worker.fetch_upload_connections.len() >= MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS {
        return Err(NativeEngineError::limit(
            "native service worker fetch request uploads",
            MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS,
            worker.fetch_upload_connections.len().saturating_add(1),
        ));
    }
    let (upload_connection, request_body) = spawn_native_fetch_upload_stream();
    worker
        .fetch_upload_connections
        .insert(request.request_id, upload_connection);
    let worker_url = worker.script_url.clone();
    let task_loader = loader.clone();
    let task = tokio::spawn(async move {
        let mut task_loader = task_loader;
        let result = task_loader
            .fetch_request_with_body_async(
                NativeFetchRequest {
                    document_url: &worker_url,
                    href: &request.href,
                    method: request.method,
                    body: None,
                    content_type: request.content_type,
                    request_headers: request.headers,
                    credentials: request.credentials,
                    credentials_mode: Some(request.credentials_mode),
                    referrer_url: request.referrer_url,
                    referrer_policy: request.referrer_policy,
                    cors_mode: request.cors_mode,
                    redirect_mode: request.redirect_mode,
                    cache_mode: request.cache_mode,
                    timeout: request.timeout,
                    max_response_bytes: None,
                },
                request_body,
            )
            .await;
        (result, task_loader)
    });
    loop {
        if task.is_finished() {
            let (result, task_loader) = task.await.map_err(|_| NativeEngineError::Worker {
                operation: "service worker fetch request upload task".into(),
                reason: "native service worker fetch upload task terminated unexpectedly".into(),
            })?;
            cancel_service_worker_fetch_upload(worker, request.request_id);
            loader.merge_fetch_task_state(task_loader)?;
            return Ok(result);
        }

        let mut disconnected = false;
        let demand = if let Some(connection) =
            worker.fetch_upload_connections.get_mut(&request.request_id)
        {
            match connection.events.try_recv() {
                Ok(NativeFetchUploadEvent::Demand) => {
                    connection.demand_pending = true;
                    true
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => false,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    disconnected = true;
                    false
                }
            }
        } else {
            false
        };
        if disconnected {
            task.abort();
            cancel_service_worker_fetch_upload(worker, request.request_id);
            return Err(NativeEngineError::Network {
                operation: "service worker fetch request upload".into(),
                reason: "service worker fetch upload task is unavailable".into(),
            });
        }
        if demand {
            let evaluation = worker.runtime.evaluate_service_worker_fetch_upload_event(
                worker.id,
                &worker.script_url,
                request.request_id,
                &json!({"type": "demand"}),
                worker.is_module,
            )?;
            for command in evaluation.commands {
                if !process_service_worker_fetch_upload_command(
                    worker,
                    request.request_id,
                    command.clone(),
                )? {
                    pending.push_back(command);
                }
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}

pub(super) fn prepare_service_worker_fetch_request(
    worker: &NativeServiceWorker,
    command: NativeScriptCommand,
) -> Result<Option<NativeServiceWorkerFetchTaskRequest>, NativeEngineError> {
    let NativeScriptCommand::Fetch {
        request_id,
        worker_id,
        href,
        credentials,
        credentials_mode,
        referrer: _,
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
    } = command
    else {
        return Ok(None);
    };
    let Some(worker_id) = worker_id else {
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch command owner is invalid".into(),
        });
    };
    if worker_id != worker.id || request_id == 0 {
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch command owner is invalid".into(),
        });
    }
    if destination.is_some() || module_referrer.is_some() || module_type.is_some() {
        return Err(NativeEngineError::invalid(
            "service worker fetch destination",
            "service worker fetch destinations and module metadata are not supported",
        ));
    }
    let request_body = if let Some(stream_id) = upload_stream_id {
        if stream_id != request_id {
            return Err(NativeEngineError::invalid(
                "service worker fetch request upload stream",
                "upload stream identifier must match its fetch request",
            ));
        }
        if body.is_some() || body_base64.is_some() {
            return Err(NativeEngineError::invalid(
                "service worker fetch request upload stream",
                "streaming fetch requests must not also carry a buffered body",
            ));
        }
        None
    } else {
        match body_base64 {
            Some(encoded) => Some(NativeRequestBody::Bytes(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "service worker fetch body",
                            "must be valid base64",
                        )
                    })?,
            )),
            None => body.map(NativeRequestBody::Text),
        }
    };
    if request_body
        .as_ref()
        .is_some_and(|body| body.len() > MAX_NATIVE_FORM_BODY_BYTES)
    {
        return Err(NativeEngineError::limit(
            "service worker fetch body",
            MAX_NATIVE_FORM_BODY_BYTES,
            request_body.as_ref().map_or(0, NativeRequestBody::len),
        ));
    }
    let method = NativeFetchMethod::from_fetch_method(&method)?;
    let cors_mode = parse_cors_mode(mode.as_deref().unwrap_or("same-origin"))?;
    let redirect_mode = parse_redirect_mode(redirect.as_deref().unwrap_or("follow"))?;
    let cache_mode = NativeFetchCacheMode::from_option(cache.as_deref())?;
    let credentials_mode = credentials_mode
        .as_deref()
        .map(NativeFetchCredentialsMode::parse)
        .transpose()?
        .unwrap_or(if credentials {
            NativeFetchCredentialsMode::Include
        } else {
            NativeFetchCredentialsMode::Omit
        });
    Ok(Some(NativeServiceWorkerFetchTaskRequest {
        request_id,
        worker_id,
        href,
        credentials,
        credentials_mode,
        referrer_url,
        referrer_policy,
        method,
        headers,
        body: request_body,
        content_type,
        cors_mode,
        redirect_mode,
        cache_mode,
        timeout: timeout_ms.map(|value| Duration::from_millis(u64::from(value))),
        upload_stream_id,
    }))
}

pub(super) async fn fetch_service_worker_request(
    loader: &mut NativeResourceLoader,
    worker_url: &str,
    request: NativeServiceWorkerFetchTaskRequest,
) -> Value {
    let response = loader
        .fetch_request_with_headers_async(NativeFetchRequest {
            document_url: worker_url,
            href: &request.href,
            method: request.method,
            body: request.body,
            content_type: request.content_type,
            request_headers: request.headers,
            credentials: request.credentials,
            credentials_mode: Some(request.credentials_mode),
            referrer_url: request.referrer_url,
            referrer_policy: request.referrer_policy,
            cors_mode: request.cors_mode,
            redirect_mode: request.redirect_mode,
            cache_mode: request.cache_mode,
            timeout: request.timeout,
            max_response_bytes: None,
        })
        .await;

    match response {
        Ok(response) => service_worker_fetch_payload(response),
        Err(error) => json!({
            "error": error.to_string(),
            "timeout": false,
        }),
    }
}

pub(super) fn schedule_service_worker_lifetime_fetch(
    worker: &NativeServiceWorker,
    loader: &NativeResourceLoader,
    command: NativeScriptCommand,
    fallback_url: &str,
    tasks: &mut JoinSet<NativeServiceWorkerFetchTaskResult>,
    abort_handles: &mut BTreeMap<(u32, u32), AbortHandle>,
    parent_broker_available: bool,
) -> Result<bool, NativeEngineError> {
    let Some(request) = prepare_service_worker_fetch_request(worker, command)? else {
        return Ok(false);
    };
    if request.upload_stream_id.is_some() {
        return Ok(false);
    }
    if tasks.len() >= MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native Service Worker lifetime fetch tasks",
            MAX_NATIVE_EFFECTS,
            tasks.len().saturating_add(1),
        ));
    }
    let key = (request.worker_id, request.request_id);
    if abort_handles.contains_key(&key) {
        return Err(NativeEngineError::Worker {
            operation: "schedule service worker lifetime fetch".into(),
            reason: "fetch request identifier is already active".into(),
        });
    }
    let worker_id = request.worker_id;
    let request_id = request.request_id;
    let fallback_url = fallback_url.to_owned();
    let abort = if parent_broker_available {
        tasks.spawn(async move {
            NativeServiceWorkerFetchTaskResult::ParentBrokered(Box::new(
                NativeServiceWorkerParentFetchTask {
                    worker_id,
                    request_id,
                    fallback_url,
                    request,
                },
            ))
        })
    } else if is_parent_owned_network_target(&worker.script_url, &request.href) {
        let error = missing_parent_network_authority("Service Worker lifetime Fetch").to_string();
        tasks.spawn(async move {
            NativeServiceWorkerFetchTaskResult::Completed(Box::new(
                NativeServiceWorkerFetchTaskCompletion {
                    worker_id,
                    request_id,
                    fallback_url,
                    payload: json!({"error": error, "timeout": false}),
                    loader: None,
                },
            ))
        })
    } else {
        let worker_url = worker.script_url.clone();
        let mut task_loader = loader.clone();
        tasks.spawn(async move {
            let payload =
                fetch_service_worker_request(&mut task_loader, &worker_url, request).await;
            NativeServiceWorkerFetchTaskResult::Completed(Box::new(
                NativeServiceWorkerFetchTaskCompletion {
                    worker_id,
                    request_id,
                    fallback_url,
                    payload,
                    loader: Some(task_loader),
                },
            ))
        })
    };
    abort_handles.insert(key, abort);
    Ok(true)
}

pub(super) struct NativeServiceWorkerFetchCommandState<'a> {
    pending: &'a mut VecDeque<NativeScriptCommand>,
    awaiting: &'a mut bool,
    value: &'a mut Value,
}

pub(super) async fn resolve_service_worker_fetch_command(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    command: NativeScriptCommand,
    state: NativeServiceWorkerFetchCommandState<'_>,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    allow_captured_load_fetch: bool,
) -> Result<bool, NativeEngineError> {
    let NativeServiceWorkerFetchCommandState {
        pending,
        awaiting,
        value,
    } = state;
    let Some(request) = prepare_service_worker_fetch_request(worker, command)? else {
        return Ok(false);
    };
    let request_id = request.request_id;
    let response = if let Some(parent_fetch_broker) = parent_fetch_broker {
        let body = if request.upload_stream_id.is_some() {
            collect_service_worker_fetch_upload(worker, request_id, pending)
                .await?
                .map(|body| Some(NativeRequestBody::Bytes(body)))
        } else {
            Ok(request.body)
        };
        match body {
            Ok(body) => {
                let worker_url = worker.script_url.clone();
                let top_level_navigation = request.cors_mode == NativeCorsMode::Navigation;
                let fetch_request = NativeFetchRequest {
                    document_url: &worker_url,
                    href: &request.href,
                    method: request.method,
                    body,
                    content_type: request.content_type,
                    request_headers: request.headers,
                    credentials: request.credentials,
                    credentials_mode: Some(request.credentials_mode),
                    referrer_url: request.referrer_url,
                    referrer_policy: request.referrer_policy,
                    cors_mode: request.cors_mode,
                    redirect_mode: request.redirect_mode,
                    cache_mode: request.cache_mode,
                    timeout: request.timeout,
                    max_response_bytes: None,
                };
                // Only the captured navigation turn may forward the trusted
                // FetchEvent navigation request through the parent's broker.
                let parent_response = if allow_captured_load_fetch {
                    parent_fetch_broker
                        .fetch_for_captured_load(request_id, &fetch_request, top_level_navigation)
                        .await?
                } else {
                    parent_fetch_broker
                        .fetch(request_id, &fetch_request)
                        .await?
                };
                parent_response.0
            }
            Err(error) => Err(error),
        }
    } else if is_parent_owned_network_target(&worker.script_url, &request.href) {
        if request.upload_stream_id.is_some() {
            let _ = collect_service_worker_fetch_upload(worker, request_id, pending).await?;
        }
        Err(missing_parent_network_authority(
            "Service Worker Fetch request",
        ))
    } else if request.upload_stream_id.is_some() {
        open_service_worker_fetch_upload(
            worker,
            loader,
            NativeServiceWorkerFetchUploadRequest {
                request_id: request.request_id,
                href: request.href,
                method: request.method,
                headers: request.headers,
                content_type: request.content_type,
                credentials: request.credentials,
                credentials_mode: request.credentials_mode,
                referrer_url: request.referrer_url,
                referrer_policy: request.referrer_policy,
                cors_mode: request.cors_mode,
                redirect_mode: request.redirect_mode,
                cache_mode: request.cache_mode,
                timeout: request.timeout,
            },
            pending,
        )
        .await?
    } else {
        let payload = fetch_service_worker_request(loader, &worker.script_url, request).await;
        let resolved = worker.runtime.resolve_service_worker_fetch(
            worker.id,
            &worker.script_url,
            request_id,
            &payload,
            worker.is_module,
        )?;
        pending.extend(resolved.commands);
        *awaiting |= resolved.top_level_await_pending;
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            *value = resolved_value;
            *awaiting = false;
        }
        return Ok(true);
    };
    let payload = match response {
        Ok(response) => service_worker_fetch_payload(response),
        Err(error) => json!({
            "error": error.to_string(),
            "timeout": false,
        }),
    };
    let resolved = worker.runtime.resolve_service_worker_fetch(
        worker.id,
        &worker.script_url,
        request.request_id,
        &payload,
        worker.is_module,
    )?;
    pending.extend(resolved.commands);
    *awaiting |= resolved.top_level_await_pending;
    if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
        *value = resolved_value;
        *awaiting = false;
    }
    Ok(true)
}

pub(super) fn apply_service_worker_cache_command(
    worker: &mut NativeServiceWorker,
    command: NativeScriptCommand,
    cache_state: &mut NativeServiceWorkerCacheState,
) -> Result<(u32, Value), NativeEngineError> {
    let worker_origin = NativeOrigin::from_url(&parse_network_url(
        "service worker cache owner URL",
        &worker.script_url,
    )?)?
    .serialized();
    let request_id = match &command {
        NativeScriptCommand::ServiceWorkerCacheOpen { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheDelete { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheHas { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheKeys { request_id }
        | NativeScriptCommand::ServiceWorkerCacheMatch { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCachePut { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCachePutAll { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheEntries { request_id, .. } => *request_id,
        _ => {
            return Err(NativeEngineError::Worker {
                operation: "service worker cache event".into(),
                reason: "service worker emitted an unsupported host command".into(),
            });
        }
    };
    if request_id == 0 {
        return Err(NativeEngineError::invalid(
            "native service worker cache request id",
            "must be positive",
        ));
    }
    let cache_name = match &command {
        NativeScriptCommand::ServiceWorkerCacheOpen { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheDelete { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheHas { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheMatch { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCachePut { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCachePutAll { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheEntries { cache_name, .. } => cache_name,
        NativeScriptCommand::ServiceWorkerCacheKeys { .. } => "",
        _ => unreachable!(),
    };
    if !cache_name.is_empty() && (cache_name.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES) {
        return Err(NativeEngineError::limit(
            "native service worker cache name",
            MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES,
            cache_name.len(),
        ));
    }
    let previous_state = cache_state.clone();
    let caches = cache_state
        .origins
        .entry(worker_origin.clone())
        .or_default();
    match command {
        NativeScriptCommand::ServiceWorkerCacheOpen { cache_name, .. } => {
            if cache_name.is_empty() {
                return Err(NativeEngineError::invalid(
                    "native service worker cache name",
                    "must not be empty",
                ));
            }
            if !caches.contains_key(&cache_name) && caches.len() >= MAX_NATIVE_SERVICE_WORKER_CACHES
            {
                return Err(NativeEngineError::limit(
                    "native service worker caches",
                    MAX_NATIVE_SERVICE_WORKER_CACHES,
                    caches.len().saturating_add(1),
                ));
            }
            caches.entry(cache_name).or_default();
            cache_state.validate()?;
            Ok((request_id, json!({"ok":true})))
        }
        NativeScriptCommand::ServiceWorkerCacheDelete { cache_name, .. } => {
            let deleted = caches.remove(&cache_name).is_some();
            if caches.is_empty() {
                cache_state.origins.remove(&worker_origin);
            }
            Ok((request_id, json!({"deleted":deleted})))
        }
        NativeScriptCommand::ServiceWorkerCacheHas { cache_name, .. } => {
            Ok((request_id, json!({"has":caches.contains_key(&cache_name)})))
        }
        NativeScriptCommand::ServiceWorkerCacheKeys { .. } => Ok((
            request_id,
            json!({"keys":caches.keys().cloned().collect::<Vec<_>>() }),
        )),
        NativeScriptCommand::ServiceWorkerCacheMatch {
            cache_name,
            request_url,
            request_method,
            request_headers,
            ignore_search,
            ignore_method,
            ignore_vary,
            ..
        } => {
            validate_native_service_worker_cache_request_headers(&request_headers)?;
            let request = validate_cache_request(&request_url, &request_method)?;
            let response = caches
                .get(&cache_name)
                .into_iter()
                .flat_map(|entries| entries.values())
                .find(|entry| {
                    cache_entry_matches(
                        entry,
                        &request,
                        &request_headers,
                        ignore_search,
                        ignore_method,
                        ignore_vary,
                    )
                })
                .map(cache_entry_payload);
            Ok((
                request_id,
                json!({"found":response.is_some(),"response":response}),
            ))
        }
        NativeScriptCommand::ServiceWorkerCachePut {
            cache_name,
            request_url,
            request_method,
            request_headers,
            response,
            ..
        } => {
            let request = validate_cache_request(&request_url, &request_method)?;
            if request.method != "GET" {
                return Err(NativeEngineError::invalid(
                    "native service worker cache request method",
                    "Cache.put only supports GET requests",
                ));
            }
            let entry = cache_entry_from_payload(&response, &request.url, request_headers)?;
            {
                let entries = caches.entry(cache_name).or_default();
                let key = cache_entry_key(&request.method, &request.url);
                if !entries.contains_key(&key)
                    && entries.len() >= MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES
                {
                    return Err(NativeEngineError::limit(
                        "native service worker cache entries",
                        MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
                        entries.len().saturating_add(1),
                    ));
                }
                entries.insert(key, entry);
            }
            if let Err(error) = cache_state.validate() {
                *cache_state = previous_state;
                return Err(error);
            }
            Ok((request_id, json!({"ok":true})))
        }
        NativeScriptCommand::ServiceWorkerCachePutAll {
            cache_name,
            entries: batch_entries,
            ..
        } => {
            let mut staged_entries = caches.get(&cache_name).cloned().unwrap_or_default();
            for batch_entry in batch_entries {
                let NativeServiceWorkerCacheBatchEntry {
                    request_url,
                    request_method,
                    request_headers,
                    response,
                } = batch_entry;
                let request = validate_cache_request(&request_url, &request_method)?;
                if request.method != "GET" {
                    return Err(NativeEngineError::invalid(
                        "native service worker cache request method",
                        "Cache.addAll only supports GET requests",
                    ));
                }
                let entry = cache_entry_from_payload(&response, &request.url, request_headers)?;
                let key = cache_entry_key(&request.method, &request.url);
                if !staged_entries.contains_key(&key)
                    && staged_entries.len() >= MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES
                {
                    return Err(NativeEngineError::limit(
                        "native service worker cache entries",
                        MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
                        staged_entries.len().saturating_add(1),
                    ));
                }
                staged_entries.insert(key, entry);
            }
            caches.insert(cache_name, staged_entries);
            if let Err(error) = cache_state.validate() {
                *cache_state = previous_state;
                return Err(error);
            }
            Ok((request_id, json!({"ok":true})))
        }
        NativeScriptCommand::ServiceWorkerCacheDeleteRequest {
            cache_name,
            request_url,
            request_method,
            request_headers,
            ignore_search,
            ignore_method,
            ignore_vary,
            ..
        } => {
            validate_native_service_worker_cache_request_headers(&request_headers)?;
            let request = validate_cache_request(&request_url, &request_method)?;
            let mut deleted = false;
            if let Some(entries) = caches.get_mut(&cache_name) {
                let keys = entries
                    .iter()
                    .filter(|(_, entry)| {
                        cache_entry_matches(
                            entry,
                            &request,
                            &request_headers,
                            ignore_search,
                            ignore_method,
                            ignore_vary,
                        )
                    })
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                for key in keys {
                    deleted |= entries.remove(&key).is_some();
                }
            }
            Ok((request_id, json!({"deleted":deleted})))
        }
        NativeScriptCommand::ServiceWorkerCacheEntries {
            cache_name,
            request_url,
            request_method,
            request_headers,
            ignore_search,
            ignore_method,
            ignore_vary,
            ..
        } => {
            validate_native_service_worker_cache_request_headers(&request_headers)?;
            let request = match (request_url, request_method) {
                (Some(request_url), Some(request_method)) => {
                    Some(validate_cache_request(&request_url, &request_method)?)
                }
                (None, None) => None,
                _ => {
                    return Err(NativeEngineError::invalid(
                        "native service worker cache keys request",
                        "URL and method must be supplied together",
                    ));
                }
            };
            let entries = caches
                .get(&cache_name)
                .into_iter()
                .flat_map(|entries| entries.values())
                .filter(|entry| {
                    request.as_ref().is_none_or(|request| {
                        cache_entry_matches(
                            entry,
                            request,
                            &request_headers,
                            ignore_search,
                            ignore_method,
                            ignore_vary,
                        )
                    })
                })
                .map(|entry| {
                    json!({
                        "url": entry.request_url,
                        "method": entry.method,
                        "headers": entry.request_headers,
                    })
                })
                .collect::<Vec<_>>();
            Ok((request_id, json!({"entries":entries})))
        }
        _ => unreachable!(),
    }
}

struct NativeServiceWorkerCacheRequest {
    url: String,
    method: String,
}

fn validate_cache_request(
    url: &str,
    method: &str,
) -> Result<NativeServiceWorkerCacheRequest, NativeEngineError> {
    let method = method.to_ascii_uppercase();
    if !matches!(
        method.as_str(),
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
    ) {
        return Err(NativeEngineError::invalid(
            "native service worker cache request method",
            "method is unsupported",
        ));
    }
    let url = without_fragment(
        parse_network_url("native service worker cache request URL", url)?.as_str(),
    )
    .to_owned();
    if url.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker cache request URL",
            MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES,
            url.len(),
        ));
    }
    Ok(NativeServiceWorkerCacheRequest { url, method })
}

pub(super) fn cache_entry_key(method: &str, url: &str) -> String {
    format!("{method}\n{url}")
}

pub(super) fn cache_match_url(url: &str, ignore_search: bool) -> Result<String, NativeEngineError> {
    let mut parsed = parse_network_url("native service worker cache match URL", url)?;
    if ignore_search {
        parsed.set_query(None);
    }
    Ok(without_fragment(parsed.as_str()).to_owned())
}

pub(super) fn cache_header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    let values = headers
        .iter()
        .filter(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.join(", "))
}

pub(super) fn cache_entry_vary_matches(
    entry: &NativeServiceWorkerCacheEntry,
    request_headers: &[(String, String)],
    ignore_vary: bool,
) -> bool {
    if ignore_vary {
        return true;
    }
    for (_, value) in entry
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("vary"))
    {
        for field in value.split(',').map(str::trim) {
            if field == "*" {
                return false;
            }
            if field.is_empty() {
                continue;
            }
            if cache_header_value(&entry.request_headers, field)
                != cache_header_value(request_headers, field)
            {
                return false;
            }
        }
    }
    true
}

fn cache_entry_matches(
    entry: &NativeServiceWorkerCacheEntry,
    request: &NativeServiceWorkerCacheRequest,
    request_headers: &[(String, String)],
    ignore_search: bool,
    ignore_method: bool,
    ignore_vary: bool,
) -> bool {
    if !ignore_method && entry.method != request.method {
        return false;
    }
    let Ok(entry_url) = cache_match_url(&entry.request_url, ignore_search) else {
        return false;
    };
    let Ok(request_url) = cache_match_url(&request.url, ignore_search) else {
        return false;
    };
    entry_url == request_url && cache_entry_vary_matches(entry, request_headers, ignore_vary)
}

pub(super) fn cache_entry_from_payload(
    value: &Value,
    fallback_url: &str,
    request_headers: Vec<(String, String)>,
) -> Result<NativeServiceWorkerCacheEntry, NativeEngineError> {
    validate_native_service_worker_cache_request_headers(&request_headers)?;
    if value.get("opaque").and_then(Value::as_bool) == Some(true)
        || value.get("opaqueRedirect").and_then(Value::as_bool) == Some(true)
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "opaque service worker responses cannot be persisted in native CacheStorage"
                .into(),
        });
    }
    let response = decode_service_worker_response(value, fallback_url)?;
    if response.status == 206 {
        return Err(NativeEngineError::invalid(
            "native service worker cached response status",
            "Cache.put and Cache.addAll must reject 206 Partial Content",
        ));
    }
    if response.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("vary") && value.split(',').any(|field| field.trim() == "*")
    }) {
        return Err(NativeEngineError::invalid(
            "native service worker cached response Vary header",
            "Cache.put and Cache.addAll must reject Vary: *",
        ));
    }
    if response.body.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker cached response body",
            MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES,
            response.body.len(),
        ));
    }
    let url = if response.url.trim().is_empty() {
        fallback_url
    } else {
        &response.url
    };
    let url = without_fragment(url).to_owned();
    Ok(NativeServiceWorkerCacheEntry {
        method: "GET".into(),
        request_url: without_fragment(fallback_url).to_owned(),
        request_headers,
        url,
        status: response.status,
        status_text: value
            .get("statusText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        headers: response.headers,
        content_type: response.content_type,
        body_base64: base64::engine::general_purpose::STANDARD.encode(response.body),
        body_null: value
            .get("bodyNull")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        redirected: response.redirected,
        opaque: false,
        opaque_redirect: false,
    })
}

pub(super) fn cache_entry_payload(entry: &NativeServiceWorkerCacheEntry) -> Value {
    json!({
        "url": entry.url,
        "status": entry.status,
        "statusText": entry.status_text,
        "headers": entry.headers,
        "contentType": entry.content_type,
        "bodyBase64": entry.body_base64,
        "bodyNull": entry.body_null,
        "redirected": entry.redirected,
        "opaque": entry.opaque,
        "opaqueRedirect": entry.opaque_redirect,
    })
}

#[cfg(test)]
pub(super) async fn settle_service_worker_fetch(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    evaluation: NativeScriptEvaluation,
    cache_state: &mut NativeServiceWorkerCacheState,
    context: NativeServiceWorkerFetchTestContext<'_>,
) -> Result<NativeServiceWorkerFetchSettlement, NativeEngineError> {
    let NativeServiceWorkerFetchTestContext {
        current_client_id,
        fallback_url,
        pending_open_windows,
        lifetime_fetch_tasks,
        lifetime_fetch_abort_handles,
    } = context;
    settle_service_worker_fetch_with_parent_fetch_broker(
        worker,
        loader,
        evaluation,
        cache_state,
        NativeServiceWorkerFetchSettlementContext {
            current_client_id,
            fallback_url,
            pending_open_windows,
            lifetime_fetch_tasks,
            lifetime_fetch_abort_handles,
            authority: NativeServiceWorkerFetchAuthority {
                parent_fetch_broker: None,
                allow_captured_load_fetch: false,
            },
        },
    )
    .await
}

pub(super) async fn settle_service_worker_fetch_with_parent_fetch_broker<'borrow, 'broker>(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    evaluation: NativeScriptEvaluation,
    cache_state: &mut NativeServiceWorkerCacheState,
    context: NativeServiceWorkerFetchSettlementContext<'borrow, 'broker>,
) -> Result<NativeServiceWorkerFetchSettlement, NativeEngineError> {
    let NativeServiceWorkerFetchSettlementContext {
        current_client_id: _current_client_id,
        fallback_url,
        pending_open_windows,
        lifetime_fetch_tasks,
        lifetime_fetch_abort_handles,
        authority,
    } = context;
    let NativeServiceWorkerFetchAuthority {
        mut parent_fetch_broker,
        allow_captured_load_fetch,
    } = authority;
    let mut pending = VecDeque::from(evaluation.commands);
    let mut value = evaluation.value;
    let mut awaiting = evaluation.top_level_await_pending;
    let mut client_messages = Vec::new();
    let mut open_window_request_id = None;
    let mut resolved_fetches = 0usize;
    if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
        value = resolved_value;
        awaiting = false;
    }
    while let Some(command) = pending.pop_front() {
        resolved_fetches = resolved_fetches.saturating_add(1);
        if resolved_fetches > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "service worker fetch event turns",
                MAX_NATIVE_MODULE_IMPORTS,
                resolved_fetches,
            ));
        }
        if !awaiting
            && matches!(
                &command,
                NativeScriptCommand::Fetch {
                    upload_stream_id: None,
                    ..
                }
            )
        {
            if !schedule_service_worker_lifetime_fetch(
                worker,
                loader,
                command,
                fallback_url,
                lifetime_fetch_tasks,
                lifetime_fetch_abort_handles,
                parent_fetch_broker.is_some(),
            )? {
                return Err(NativeEngineError::Worker {
                    operation: "schedule service worker lifetime fetch".into(),
                    reason: "fetch task was not eligible for asynchronous execution".into(),
                });
            }
            continue;
        }
        if apply_service_worker_lifecycle_command(worker, &command)? {
            continue;
        }
        if let Some(message) = service_worker_client_message_command(worker.id, command.clone())? {
            client_messages.push(message);
            continue;
        }
        if let Some(request) = service_worker_open_window_command(worker, command.clone())? {
            if pending_open_windows.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native service worker openWindow requests",
                    MAX_NATIVE_EFFECTS,
                    pending_open_windows.len().saturating_add(1),
                ));
            }
            open_window_request_id.get_or_insert(request.request_id);
            pending_open_windows.push_back(request);
            continue;
        }
        if is_service_worker_cache_command(&command) {
            let (request_id, payload) =
                apply_service_worker_cache_command(worker, command, cache_state)?;
            let resolved = worker.runtime.resolve_service_worker_cache(
                worker.id,
                &worker.script_url,
                request_id,
                &payload,
                worker.is_module,
            )?;
            pending.extend(resolved.commands);
            awaiting |= resolved.top_level_await_pending;
            if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
                value = resolved_value;
                awaiting = false;
            }
            continue;
        }
        if !resolve_service_worker_fetch_command(
            worker,
            loader,
            command,
            NativeServiceWorkerFetchCommandState {
                pending: &mut pending,
                awaiting: &mut awaiting,
                value: &mut value,
            },
            parent_fetch_broker.as_deref_mut(),
            allow_captured_load_fetch,
        )
        .await?
        {
            return Err(NativeEngineError::Worker {
                operation: "service worker fetch event".into(),
                reason: "service worker emitted an unsupported host command".into(),
            });
        }
    }
    if awaiting {
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            return Ok(NativeServiceWorkerFetchSettlement::Complete {
                value: resolved_value,
                client_messages,
            });
        }
        if let Some(open_window_request_id) = open_window_request_id {
            return Ok(NativeServiceWorkerFetchSettlement::Suspended {
                continuation: NativeServiceWorkerFetchContinuation {
                    worker_id: worker.id,
                    open_window_request_id,
                    fallback_url: fallback_url.to_owned(),
                    pending,
                    value,
                    awaiting,
                },
                client_messages,
            });
        }
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch response promise remained pending".into(),
        });
    }
    Ok(NativeServiceWorkerFetchSettlement::Complete {
        value,
        client_messages,
    })
}

pub(super) fn apply_service_worker_lifecycle_command(
    worker: &mut NativeServiceWorker,
    command: &NativeScriptCommand,
) -> Result<bool, NativeEngineError> {
    let worker_id = match command {
        NativeScriptCommand::ServiceWorkerSkipWaiting { worker_id }
        | NativeScriptCommand::ServiceWorkerClientsClaim { worker_id } => *worker_id,
        _ => return Ok(false),
    };
    if worker_id == 0 || worker_id != worker.id {
        return Err(NativeEngineError::Worker {
            operation: "service worker lifecycle command".into(),
            reason: "service worker lifecycle command owner is invalid".into(),
        });
    }
    if matches!(
        command,
        NativeScriptCommand::ServiceWorkerSkipWaiting { .. }
    ) {
        worker.skip_waiting_requested = true;
    } else {
        worker.clients_claim_requested = true;
    }
    Ok(true)
}

pub(super) fn is_service_worker_cache_command(command: &NativeScriptCommand) -> bool {
    matches!(
        command,
        NativeScriptCommand::ServiceWorkerCacheOpen { .. }
            | NativeScriptCommand::ServiceWorkerCacheDelete { .. }
            | NativeScriptCommand::ServiceWorkerCacheHas { .. }
            | NativeScriptCommand::ServiceWorkerCacheKeys { .. }
            | NativeScriptCommand::ServiceWorkerCacheMatch { .. }
            | NativeScriptCommand::ServiceWorkerCachePut { .. }
            | NativeScriptCommand::ServiceWorkerCachePutAll { .. }
            | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { .. }
            | NativeScriptCommand::ServiceWorkerCacheEntries { .. }
    )
}

pub(super) fn resolve_service_worker_fetch_cache_commands(
    worker: &mut NativeServiceWorker,
    evaluation: &mut NativeScriptEvaluation,
    cache_state: &mut NativeServiceWorkerCacheState,
) -> Result<(), NativeEngineError> {
    let mut pending = VecDeque::from(std::mem::take(&mut evaluation.commands));
    let mut deferred = VecDeque::new();
    let mut cache_turns = 0usize;
    while let Some(command) = pending.pop_front() {
        if !is_service_worker_cache_command(&command) {
            deferred.push_back(command);
            deferred.append(&mut pending);
            break;
        }
        cache_turns = cache_turns.saturating_add(1);
        if cache_turns > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "native ServiceWorker FetchEvent CacheStorage turns",
                MAX_NATIVE_MODULE_IMPORTS,
                cache_turns,
            ));
        }
        let (request_id, payload) =
            apply_service_worker_cache_command(worker, command, cache_state)?;
        let resolved = worker.runtime.resolve_service_worker_cache(
            worker.id,
            &worker.script_url,
            request_id,
            &payload,
            worker.is_module,
        )?;
        let command_count = deferred
            .len()
            .saturating_add(pending.len())
            .saturating_add(resolved.commands.len());
        if command_count > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native ServiceWorker FetchEvent CacheStorage commands",
                MAX_NATIVE_WORKER_MESSAGES,
                command_count,
            ));
        }
        pending.extend(resolved.commands);
        evaluation.top_level_await_pending |= resolved.top_level_await_pending;
        if evaluation.worker_script_error.is_none() {
            evaluation.worker_script_error = resolved.worker_script_error;
        }
        if let Some(value) = worker.runtime.take_top_level_await_result()? {
            evaluation.value = value;
            evaluation.top_level_await_pending = false;
        }
    }
    evaluation.commands = deferred.into_iter().collect();
    Ok(())
}

pub(super) fn service_worker_fetch_payload(response: NativeFetchResponse) -> Value {
    json!({
        "url": response.url,
        "status": response.status,
        "statusText": response.status_text,
        "contentType": response.content_type,
        "headers": response.headers,
        "body": String::from_utf8_lossy(&response.body),
        "bodyBase64": base64::engine::general_purpose::STANDARD.encode(&response.body),
        "bodyNull": false,
        "redirected": response.redirected,
        "opaque": response.opaque,
        "opaqueRedirect": response.opaque_redirect,
    })
}

pub(super) fn resolve_payload_for_preload(
    preload_result: Option<&Result<NativeFetchResponse, NativeEngineError>>,
) -> Value {
    match preload_result {
        Some(Ok(response)) => service_worker_fetch_payload(response.clone()),
        Some(Err(error)) => {
            let message = error.to_string().chars().take(1024).collect::<String>();
            json!({"error": message})
        }
        None => json!({
            "error": "navigation preload was aborted after the ServiceWorker response settled"
        }),
    }
}

pub(super) fn resolve_service_worker_navigation_preload(
    runtime: &NativeJavaScriptRuntime,
    worker_id: u32,
    worker_url: &str,
    request_id: u32,
    preload_result: Option<&Result<NativeFetchResponse, NativeEngineError>>,
    is_module: bool,
) -> Result<NativeScriptEvaluation, NativeEngineError> {
    runtime.resolve_service_worker_navigation_preload(
        worker_id,
        worker_url,
        request_id,
        &resolve_payload_for_preload(preload_result),
        is_module,
    )
}

pub(super) fn append_service_worker_fetch_evaluation(
    evaluation: &mut NativeScriptEvaluation,
    additional: NativeScriptEvaluation,
    operation: &'static str,
) -> Result<(), NativeEngineError> {
    let command_count = evaluation
        .commands
        .len()
        .saturating_add(additional.commands.len());
    if command_count > MAX_NATIVE_WORKER_MESSAGES {
        return Err(NativeEngineError::limit(
            operation,
            MAX_NATIVE_WORKER_MESSAGES,
            command_count,
        ));
    }
    evaluation.commands.extend(additional.commands);
    evaluation.top_level_await_pending |= additional.top_level_await_pending;
    if evaluation.worker_script_error.is_none() {
        evaluation.worker_script_error = additional.worker_script_error;
    }
    Ok(())
}

pub(super) fn take_service_worker_fetch_response(
    worker: &NativeServiceWorker,
    evaluation: &mut NativeScriptEvaluation,
) -> Result<bool, NativeEngineError> {
    if !evaluation.top_level_await_pending {
        return Ok(true);
    }
    if let Some(value) = worker.runtime.take_top_level_await_result()? {
        evaluation.value = value;
        evaluation.top_level_await_pending = false;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn run_service_worker_fetch_timer_turn(
    worker: &NativeServiceWorker,
    evaluation: &mut NativeScriptEvaluation,
    timer_turns: &mut usize,
) -> Result<bool, NativeEngineError> {
    *timer_turns = timer_turns.saturating_add(1);
    if *timer_turns > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native ServiceWorker FetchEvent timer turns",
            MAX_NATIVE_EFFECTS,
            *timer_turns,
        ));
    }
    let timer_evaluation = worker.runtime.evaluate_service_worker_timers(
        worker.id,
        &worker.script_url,
        worker.is_module,
    )?;
    append_service_worker_fetch_evaluation(
        evaluation,
        timer_evaluation,
        "native ServiceWorker FetchEvent timer commands",
    )?;
    take_service_worker_fetch_response(worker, evaluation)
}

pub(super) fn decode_service_worker_response(
    value: &Value,
    fallback_url: &str,
) -> Result<NativeFetchResponse, NativeEngineError> {
    let status = value
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode service worker response".into(),
            reason: "service worker response status was invalid".into(),
        })?;
    if !(200..=599).contains(&status) {
        return Err(NativeEngineError::Worker {
            operation: "decode service worker response".into(),
            reason: "service worker response status was outside the supported range".into(),
        });
    }
    let body = if value.get("bodyNull").and_then(Value::as_bool) == Some(true) {
        Vec::new()
    } else if let Some(encoded) = value.get("bodyBase64").and_then(Value::as_str) {
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker response body was not valid base64".into(),
            })?
    } else {
        value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .as_bytes()
            .to_vec()
    };
    if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
        return Err(NativeEngineError::limit(
            "service worker response body",
            MAX_NATIVE_FORM_BODY_BYTES,
            body.len(),
        ));
    }
    let headers = value
        .get("headers")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode service worker response".into(),
            reason: "service worker response headers were invalid".into(),
        })?
        .iter()
        .map(|entry| {
            let values = entry.as_array().ok_or_else(|| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker response header entry was invalid".into(),
            })?;
            if values.len() != 2 {
                return Err(NativeEngineError::Worker {
                    operation: "decode service worker response".into(),
                    reason: "service worker response header entry had the wrong arity".into(),
                });
            }
            Ok((
                values[0].as_str().unwrap_or_default().to_owned(),
                values[1].as_str().unwrap_or_default().to_owned(),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if headers.len() > 64 {
        return Err(NativeEngineError::limit(
            "service worker response headers",
            64,
            headers.len(),
        ));
    }
    Ok(NativeFetchResponse {
        url: value
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .unwrap_or(fallback_url)
            .to_owned(),
        status,
        status_text: value
            .get("statusText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        content_type: value
            .get("contentType")
            .and_then(|value| (!value.is_null()).then_some(value))
            .and_then(Value::as_str)
            .map(str::to_owned),
        headers,
        body,
        redirected: value
            .get("redirected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        opaque: false,
        opaque_redirect: false,
    })
}

pub(super) fn parse_network_url(field: &str, value: &str) -> Result<Url, NativeEngineError> {
    validate_url_text(field, value)?;
    let url =
        Url::parse(without_fragment(value)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must be a valid HTTP(S) URL"),
        })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must use HTTP(S)"),
        });
    }
    if url.username() != "" || url.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must not contain credentials"),
        });
    }
    Ok(url)
}

pub(super) fn resolve_same_origin_url(
    owner: &Url,
    field: &str,
    value: &str,
) -> Result<Url, NativeEngineError> {
    validate_url_text(field, value)?;
    let url = owner
        .join(value)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{field} is not valid URL syntax"),
        })?;
    let url = parse_network_url(field, url.as_str())?;
    if NativeOrigin::from_url(&url)? != NativeOrigin::from_url(owner)? {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must be same-origin"),
        });
    }
    Ok(url)
}

pub(super) fn resolve_same_origin_or_cross_origin_url(
    owner: &Url,
    field: &str,
    value: &str,
) -> Result<Url, NativeEngineError> {
    validate_url_text(field, value)?;
    let url = owner
        .join(value)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{field} is not valid URL syntax"),
        })?;
    parse_network_url(field, url.as_str())
}

pub(crate) fn native_service_worker_client_id(context_id: &str, frame_id: &str) -> String {
    // The browser backend owns context/frame identity. Hash both components
    // so the client id survives content-process replacement without exposing
    // a document URL as an opaque Service Worker client id.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in context_id
        .as_bytes()
        .iter()
        .chain(std::iter::once(&0xff))
        .chain(frame_id.as_bytes().iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("native-client-{hash:016x}")
}

pub(super) fn native_service_worker_url_client_id(document_url: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in document_url.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("native-client-{hash:016x}")
}

pub(super) fn cors_mode_text(mode: NativeCorsMode) -> &'static str {
    match mode {
        NativeCorsMode::NoCors => "no-cors",
        NativeCorsMode::Cors => "cors",
        NativeCorsMode::SameOrigin => "same-origin",
        NativeCorsMode::Navigation => "same-origin",
    }
}

pub(super) fn parse_cors_mode(value: &str) -> Result<NativeCorsMode, NativeEngineError> {
    match value {
        "no-cors" => Ok(NativeCorsMode::NoCors),
        "cors" => Ok(NativeCorsMode::Cors),
        "same-origin" => Ok(NativeCorsMode::SameOrigin),
        "navigate" => Ok(NativeCorsMode::Navigation),
        _ => Err(NativeEngineError::invalid(
            "service worker fetch mode",
            "must be navigate, no-cors, cors, or same-origin",
        )),
    }
}

pub(super) fn redirect_mode_text(mode: NativeFetchRedirectMode) -> &'static str {
    match mode {
        NativeFetchRedirectMode::Follow => "follow",
        NativeFetchRedirectMode::Error => "error",
        NativeFetchRedirectMode::Manual => "manual",
    }
}

pub(super) fn parse_redirect_mode(
    value: &str,
) -> Result<NativeFetchRedirectMode, NativeEngineError> {
    match value {
        "follow" => Ok(NativeFetchRedirectMode::Follow),
        "error" => Ok(NativeFetchRedirectMode::Error),
        "manual" => Ok(NativeFetchRedirectMode::Manual),
        _ => Err(NativeEngineError::invalid(
            "service worker fetch redirect mode",
            "must be follow, error, or manual",
        )),
    }
}
