use super::*;

#[doc(hidden)]
pub async fn run_native_content_worker() -> Result<(), NativeEngineError> {
    let mut stdout = tokio::io::stdout();
    let (ipc_request_tx, mut ipc_request_rx) = mpsc::channel(1);
    let (dialog_decision_tx, dialog_decision_rx) = std::sync::mpsc::channel();
    let sync_xhr_context = Arc::new(Mutex::new(None::<(u64, NativeContentCookieOwner)>));
    let sync_xhr_replies = Arc::new(Mutex::new(BTreeMap::<
        u64,
        std::sync::mpsc::SyncSender<Vec<u8>>,
    >::new()));
    let next_sync_xhr_id = Arc::new(AtomicU64::new(1));
    let broker_context = Arc::clone(&sync_xhr_context);
    let broker_replies = Arc::clone(&sync_xhr_replies);
    let broker_next_id = Arc::clone(&next_sync_xhr_id);
    let sync_xhr_parent_broker: NativeSyncXhrParentBroker =
        Arc::new(move |encoded_request, cookie_writes| {
            if encoded_request.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(NativeEngineError::limit(
                    "parent-brokered synchronous XHR request",
                    MAX_NATIVE_SCRIPT_BYTES,
                    encoded_request.len(),
                ));
            }
            let cookie_write_bytes = cookie_writes
                .iter()
                .fold(0usize, |total, value| total.saturating_add(value.len()));
            if cookie_writes.len() > MAX_NATIVE_EFFECTS
                || cookie_write_bytes > MAX_CONTENT_DOCUMENT_COOKIE_BYTES
            {
                return Err(NativeEngineError::limit(
                    "parent-brokered synchronous XHR cookie writes",
                    MAX_NATIVE_EFFECTS,
                    cookie_writes.len(),
                ));
            }
            let (request_id, owner) = broker_context
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "parent-brokered synchronous XHR".into(),
                    reason: "active content owner is unavailable".into(),
                })?
                .clone()
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "parent-brokered synchronous XHR".into(),
                    reason: "parent cookie and network authority is unavailable".into(),
                })?;
            let request: Value = serde_json::from_str(encoded_request).map_err(|_| {
                NativeEngineError::invalid(
                    "parent-brokered synchronous XHR request",
                    "must be valid JSON",
                )
            })?;
            let sync_id = broker_next_id
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_add(1)
                })
                .map_err(|_| NativeEngineError::Worker {
                    operation: "parent-brokered synchronous XHR".into(),
                    reason: "request ID space is exhausted".into(),
                })?;
            let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
            {
                let mut replies = broker_replies
                    .lock()
                    .map_err(|_| NativeEngineError::Worker {
                        operation: "parent-brokered synchronous XHR".into(),
                        reason: "response routing is unavailable".into(),
                    })?;
                if replies.len() >= MAX_NATIVE_EFFECTS || replies.contains_key(&sync_id) {
                    return Err(NativeEngineError::limit(
                        "pending parent-brokered synchronous XHR requests",
                        MAX_NATIVE_EFFECTS,
                        replies.len().saturating_add(1),
                    ));
                }
                replies.insert(sync_id, reply_tx);
            }
            let frame = json!({
                "kind": "parent_sync_xhr_request",
                "id": request_id,
                "sync_id": sync_id,
                "owner": owner,
                "request": request,
                "cookie_writes": cookie_writes,
            });
            let payload = serde_json::to_vec(&frame).map_err(|_| NativeEngineError::Worker {
                operation: "encode parent-brokered synchronous XHR".into(),
                reason: "request could not be encoded".into(),
            })?;
            if let Err(error) = write_sync_frame(std::io::stdout(), &payload) {
                if let Ok(mut replies) = broker_replies.lock() {
                    replies.remove(&sync_id);
                }
                return Err(NativeEngineError::Worker {
                    operation: "write parent-brokered synchronous XHR".into(),
                    reason: error.to_string(),
                });
            }
            let response = reply_rx.recv().map_err(|_| NativeEngineError::Worker {
                operation: "parent-brokered synchronous XHR".into(),
                reason: "parent closed the synchronous XHR response channel".into(),
            })?;
            if let Ok(mut replies) = broker_replies.lock() {
                replies.remove(&sync_id);
            }
            let response: Value = serde_json::from_slice(&response).map_err(|_| {
                NativeEngineError::worker_failure(
                    "parent-brokered synchronous XHR",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned invalid synchronous XHR JSON",
                )
            })?;
            if response.get("kind").and_then(Value::as_str) != Some("parent_sync_xhr_response")
                || response.get("id").and_then(Value::as_u64) != Some(request_id)
                || response.get("sync_id").and_then(Value::as_u64) != Some(sync_id)
            {
                return Err(NativeEngineError::worker_failure(
                    "parent-brokered synchronous XHR",
                    NativeWorkerFailureKind::Protocol,
                    "response does not match the active request",
                ));
            }
            let document_cookie = response
                .get("document_cookie")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent-brokered synchronous XHR",
                        NativeWorkerFailureKind::Protocol,
                        "parent omitted the visible document-cookie projection",
                    )
                })?;
            if document_cookie.len() > crate::limits::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "parent synchronous XHR cookie projection",
                    crate::limits::MAX_TEXT_BYTES,
                    document_cookie.len(),
                ));
            }
            let fetch = response.get("fetch").ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent-brokered synchronous XHR",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the XHR result",
                )
            })?;
            if fetch.get("kind").and_then(Value::as_str) == Some("error") {
                require_response_kind(
                    fetch,
                    "error",
                    request_id,
                    "parent-brokered synchronous XHR",
                )?;
            }
            let mut result = match fetch.get("kind").and_then(Value::as_str) {
                Some("fetched") => {
                    let response = decode_fetch_response(fetch, request_id)?;
                    if response.opaque || response.opaque_redirect {
                        return Err(NativeEngineError::worker_failure(
                            "parent-brokered synchronous XHR",
                            NativeWorkerFailureKind::Protocol,
                            "parent returned an opaque response for XHR",
                        ));
                    }
                    json!({
                        "error": false,
                        "url": response.url,
                        "status": response.status,
                        "statusText": response.status_text,
                        "contentType": response.content_type,
                        "headers": response.headers,
                        "bodyBase64": base64::engine::general_purpose::STANDARD
                            .encode(response.body),
                        "redirected": response.redirected,
                    })
                }
                Some("error") => {
                    let reason = fetch
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("parent rejected the synchronous XHR request");
                    json!({
                        "error": true,
                        "errorMessage": reason,
                        "timeout": reason.to_ascii_lowercase().contains("timeout"),
                    })
                }
                _ => {
                    return Err(NativeEngineError::worker_failure(
                        "parent-brokered synchronous XHR",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned an unknown XHR result",
                    ));
                }
            };
            result["documentCookie"] = Value::String(document_cookie.to_owned());
            Ok(result)
        });
    install_native_sync_xhr_parent_broker(sync_xhr_parent_broker)?;
    let dialog_rpc = Arc::new(NativeWorkerDialogRpc::new(dialog_decision_rx));
    let dialog_waiting = Arc::clone(&dialog_rpc.dialog_waiting);
    let reader_sync_xhr_replies = Arc::clone(&sync_xhr_replies);
    let _ipc_reader = std::thread::spawn(move || {
        forward_content_ipc_frames(
            std::io::stdin(),
            ipc_request_tx,
            dialog_waiting,
            dialog_decision_tx,
            reader_sync_xhr_replies,
        );
    });
    let mut running = false;
    let mut document: Option<NativeDocument> = None;
    let mut pending_space_activation: Option<(NativeNodeId, Option<NativeCheckableKind>)> = None;
    let mut document_url = None;
    let mut document_origin = None;
    let mut viewport = Viewport::default();
    let mut scroll_offset = NativePoint { x: 0, y: 0 };
    let mut nested_scroll_offsets = BTreeMap::new();
    let mut environment = NativeEnvironmentOverrides::default();
    let mut resource_loader: Option<NativeResourceLoader> = None;
    let mut javascript_runtime: Option<NativeJavaScriptRuntime> = None;
    let mut workers = NativeWorkerRegistry::new_with_fetch_streams();
    let mut service_workers = NativeServiceWorkerRegistry::default();
    let mut pending_worker_messages: VecDeque<NativeWorkerMessage> = VecDeque::new();
    let mut pending_worker_websocket_commands: VecDeque<NativeWorkerWebSocketCommand> =
        VecDeque::new();
    let mut pending_message_port_messages: VecDeque<NativeMessagePortPageMessage> = VecDeque::new();
    let mut pending_page_message_port_commands: VecDeque<NativePageMessagePortCommand> =
        VecDeque::new();
    let mut pending_shared_worker_commands: VecDeque<NativeScriptCommand> = VecDeque::new();
    let mut pending_service_worker_client_messages: VecDeque<NativeServiceWorkerClientMessage> =
        VecDeque::new();
    let mut pending_external_service_worker_client_messages: VecDeque<
        NativeServiceWorkerClientMessage,
    > = VecDeque::new();
    let mut pending_service_worker_open_windows: VecDeque<NativeServiceWorkerOpenWindowRequest> =
        VecDeque::new();
    let mut websocket_connections = BTreeMap::new();
    let mut worker_websocket_connections = BTreeMap::new();
    let mut worker_websocket_read_cursor = None;
    let mut fetch_stream_connections = BTreeMap::new();
    let mut fetch_upload_connections: BTreeMap<u32, NativeFetchUploadConnection> = BTreeMap::new();
    let mut pending_upload_fetches: BTreeMap<u32, NativePendingUploadFetch> = BTreeMap::new();
    let mut pending_controlled_uploads: BTreeMap<u32, NativePendingControlledUpload> =
        BTreeMap::new();
    let mut event_source_connections = BTreeMap::new();
    let mut worker_event_source_connections = BTreeMap::new();
    let mut worker_event_source_read_cursor = None;
    let mut storage_state = NativeWebStorageState::default();
    let mut indexed_db_state = NativeIndexedDbState::default();
    let mut pending_lifetime_cookie_changes = Vec::new();
    let mut parent_document_cookie_projection: Option<String> = None;
    let mut storage_profile_path: Option<PathBuf> = None;
    let mut allowed_file_roots: Vec<PathBuf> = Vec::new();
    let mut storage_context_id = NATIVE_CONTEXT_ID.to_owned();
    let mut frame_id = NATIVE_CONTEXT_ID.to_owned();
    let mut window_name = String::new();
    let mut opener_context_id: Option<String> = None;
    let mut opener_window_name = String::new();
    let mut opener_url = String::new();
    let mut frame_script_context: Option<NativeFrameScriptContext> = None;
    let mut frame_script_bindings = Vec::new();
    let mut pending_service_worker_navigation: Option<Value> = None;
    let mut external_shared_worker_routing = false;
    let mut next_async_effect_sequence = 1_u64;
    let mut async_effect_notification_pending = false;
    let mut cancelled_parent_fetches = BTreeSet::new();
    let dialog_rpc_for_handler = Arc::clone(&dialog_rpc);
    let dialog_handler: NativeDialogHandler =
        Arc::new(move |dialog, url| dialog_rpc_for_handler.request_dialog(dialog, url));
    loop {
        let timer_delay_ms = if resource_loader.is_some() {
            javascript_runtime
                .as_ref()
                .map(|runtime| runtime.next_timer_delay_ms())
                .transpose()?
                .flatten()
                .into_iter()
                .chain(service_workers.next_timer_delay_ms()?)
                .chain(workers.next_timer_delay_ms()?)
                .min()
        } else {
            None
        };
        let input = tokio::select! {
            request = ipc_request_rx.recv() => {
                NativeContentProcessInput::Request(request.unwrap_or_else(|| {
                    Err(NativeEngineError::Worker {
                        operation: "read content IPC".into(),
                        reason: "content process stdin reader stopped".into(),
                    })
                }))
            }
            completion = service_workers.next_lifetime_fetch_task(),
                if !async_effect_notification_pending && service_workers.has_lifetime_fetch_tasks() =>
            {
                NativeContentProcessInput::ServiceWorkerLifetimeFetch(
                    completion?.map(Box::new),
                )
            }
            _ = tokio::time::sleep(Duration::from_millis(timer_delay_ms.unwrap_or_default())),
                if !async_effect_notification_pending && timer_delay_ms.is_some() =>
            {
                NativeContentProcessInput::WorkerTimer
            }
        };
        let payload = match input {
            NativeContentProcessInput::Request(payload) => payload?,
            NativeContentProcessInput::ServiceWorkerLifetimeFetch(Some(completion)) => {
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "Service Worker lifetime fetch completion".into(),
                        reason: "content process has no resource loader".into(),
                    });
                };
                match *completion {
                    NativeServiceWorkerFetchTaskResult::Completed(completion) => {
                        service_workers
                            .resolve_lifetime_fetch_task(*completion, loader, None)
                            .await?;
                    }
                    parent_request @ NativeServiceWorkerFetchTaskResult::ParentBrokered(_) => {
                        service_workers.queue_parent_brokered_lifetime_fetch(parent_request)?;
                    }
                }
                pending_message_port_messages.extend(service_workers.take_message_port_messages());
                pending_service_worker_client_messages
                    .extend(service_workers.take_client_messages());
                pending_external_service_worker_client_messages
                    .extend(service_workers.take_external_client_messages());
                pending_service_worker_open_windows.extend(service_workers.take_open_windows());
                refresh_content_runtime_cookie(
                    javascript_runtime.as_ref(),
                    resource_loader.as_ref(),
                    document_url.as_deref(),
                )?;
                let cookie_changes = persist_content_profile(
                    storage_profile_path.as_deref(),
                    &storage_state,
                    &indexed_db_state,
                    &[],
                    &[],
                    &mut service_workers,
                    &mut resource_loader,
                )?;
                merge_cookie_change_batch(
                    &mut pending_lifetime_cookie_changes,
                    cookie_changes,
                    "Service Worker lifetime cookie changes",
                )?;
                let has_pending_effects = content_worker_effects_pending(
                    &pending_worker_messages,
                    &pending_worker_websocket_commands,
                    &pending_message_port_messages,
                    &pending_page_message_port_commands,
                    &pending_shared_worker_commands,
                    &pending_service_worker_client_messages,
                    &pending_external_service_worker_client_messages,
                    &pending_service_worker_open_windows,
                    &pending_lifetime_cookie_changes,
                )? || service_workers
                    .has_pending_parent_lifetime_fetches();
                write_async_effects_ready(
                    &mut stdout,
                    has_pending_effects,
                    &mut async_effect_notification_pending,
                    &mut next_async_effect_sequence,
                    &storage_context_id,
                    &frame_id,
                )
                .await?;
                continue;
            }
            NativeContentProcessInput::ServiceWorkerLifetimeFetch(None) => continue,
            NativeContentProcessInput::WorkerTimer => {
                // Autonomous page, DedicatedWorker, and ServiceWorker timers
                // run only in the exact page-owner turn, where Fetch and
                // resource loads have the parent's broker. A standalone
                // content process must not fall back to its local loader.
                let defer_page_timer = javascript_runtime
                    .as_ref()
                    .map(|runtime| runtime.next_timer_delay_ms())
                    .transpose()?
                    .flatten()
                    .is_some_and(|delay| delay == 0);
                let service_worker_timer_delay = service_workers.next_timer_delay_ms()?;
                let defer_service_worker_timer = service_worker_timer_delay.is_some();
                let defer_worker_timer = workers.next_timer_delay_ms()?.is_some();
                pending_worker_websocket_commands.extend(workers.take_websocket_commands());
                pending_worker_messages.extend(workers.take_messages());
                pending_message_port_messages.extend(workers.take_message_port_messages());
                pending_message_port_messages.extend(service_workers.take_message_port_messages());
                pending_page_message_port_commands
                    .extend(workers.take_page_message_port_commands());
                pending_service_worker_client_messages
                    .extend(service_workers.take_client_messages());
                pending_external_service_worker_client_messages
                    .extend(service_workers.take_external_client_messages());
                pending_service_worker_open_windows.extend(service_workers.take_open_windows());
                refresh_content_runtime_cookie(
                    javascript_runtime.as_ref(),
                    resource_loader.as_ref(),
                    document_url.as_deref(),
                )?;
                let cookie_changes = persist_content_profile(
                    storage_profile_path.as_deref(),
                    &storage_state,
                    &indexed_db_state,
                    &[],
                    &[],
                    &mut service_workers,
                    &mut resource_loader,
                )?;
                merge_cookie_change_batch(
                    &mut pending_lifetime_cookie_changes,
                    cookie_changes,
                    "worker timer cookie changes",
                )?;
                let has_pending_effects = defer_page_timer
                    || defer_service_worker_timer
                    || defer_worker_timer
                    || content_worker_effects_pending(
                        &pending_worker_messages,
                        &pending_worker_websocket_commands,
                        &pending_message_port_messages,
                        &pending_page_message_port_commands,
                        &pending_shared_worker_commands,
                        &pending_service_worker_client_messages,
                        &pending_external_service_worker_client_messages,
                        &pending_service_worker_open_windows,
                        &pending_lifetime_cookie_changes,
                    )?
                    || service_workers.has_pending_parent_lifetime_fetches();
                write_async_effects_ready(
                    &mut stdout,
                    has_pending_effects,
                    &mut async_effect_notification_pending,
                    &mut next_async_effect_sequence,
                    &storage_context_id,
                    &frame_id,
                )
                .await?;
                continue;
            }
        };
        let request: Value =
            serde_json::from_slice(&payload).map_err(|_| NativeEngineError::Worker {
                operation: "decode content IPC".into(),
                reason: "content process received an invalid request".into(),
            })?;
        if discard_cancelled_parent_fetch_frame(&request, &mut cancelled_parent_fetches)? {
            continue;
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let response_id = id.clone();
        let request_id = id.as_u64().ok_or_else(|| {
            NativeEngineError::invalid("content-process request ID", "must be an unsigned integer")
        })?;
        dialog_rpc.begin_request(request_id);
        let kind = request
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let captured_cookie_owner = request
            .get("owner")
            .map(|owner| decode_content_cookie_owner(owner, "active synchronous XHR owner"))
            .transpose()?;
        let fallback_owner_url = if kind == "load" {
            request.get("url").and_then(Value::as_str)
        } else {
            document_url.as_deref()
        };
        let fallback_owner_generation = request
            .get("generation")
            .and_then(Value::as_u64)
            .and_then(|generation| u32::try_from(generation).ok())
            .or_else(|| document.as_ref().map(NativeDocument::generation));
        let active_cookie_owner = captured_cookie_owner.or_else(|| {
            Some(NativeContentCookieOwner {
                context_id: storage_context_id.clone(),
                frame_id: frame_id.clone(),
                generation: fallback_owner_generation?,
                document_url: fallback_owner_url?.to_owned(),
            })
        });
        if let Ok(mut context) = sync_xhr_context.lock() {
            *context = active_cookie_owner.clone().map(|owner| (request_id, owner));
        }
        if kind == "script" {
            async_effect_notification_pending = false;
        }
        if let (Some(runtime), Some(loader)) =
            (javascript_runtime.as_ref(), resource_loader.as_mut())
            && let Some(updated_loader) = runtime.take_sync_xhr_loader()
        {
            loader.merge_fetch_task_state(updated_loader)?;
        }
        if let (Some(runtime), Some(loader)) =
            (javascript_runtime.as_ref(), resource_loader.as_ref())
        {
            runtime.set_sync_xhr_loader(loader);
        }
        if let (Some(runtime), Some(projection)) = (
            javascript_runtime.as_ref(),
            parent_document_cookie_projection.as_ref(),
        ) {
            runtime.set_cookie_state(projection.clone());
        } else {
            refresh_content_runtime_cookie(
                javascript_runtime.as_ref(),
                resource_loader.as_ref(),
                document_url.as_deref(),
            )?;
        }
        if let (Some(runtime), Some(document_url), Some(document_origin)) = (
            javascript_runtime.as_ref(),
            document_url.as_deref(),
            document_origin.as_ref(),
        ) {
            runtime.set_indexed_db_state(
                indexed_db_state.origin(&storage_key(document_url, document_origin)),
            );
        }
        if let Some(runtime) = javascript_runtime.as_ref() {
            runtime.set_environment(environment.clone());
        }
        let mut response = match kind {
            "ping" if protocol_matches(&request) => {
                json!({"kind":"pong","id":id,"protocol":CONTENT_WORKER_PROTOCOL_VERSION})
            }
            "start" if protocol_matches(&request) && !running => {
                let modal_dialogs = request
                    .get("modal_dialogs")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process modal dialog setting",
                            "must be boolean",
                        )
                    })?;
                dialog_rpc.set_enabled(modal_dialogs);
                external_shared_worker_routing = request
                    .get("external_shared_worker_routing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let requested_context_id = request
                    .get("context_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process context id", "must be text")
                    })?;
                validate_context_id(requested_context_id)?;
                let requested_frame_id = request
                    .get("frame_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process frame id", "must be text")
                    })?;
                validate_context_id(requested_frame_id)?;
                let requested_window_name = request
                    .get("window_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                validate_window_name(requested_window_name)?;
                let requested_opener_context_id =
                    request.get("opener_context_id").and_then(Value::as_str);
                if let Some(opener_context_id) = requested_opener_context_id {
                    validate_context_id(opener_context_id)?;
                }
                let requested_opener_window_name = request
                    .get("opener_window_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                validate_window_name(requested_opener_window_name)?;
                let requested_opener_url = request
                    .get("opener_url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !requested_opener_url.is_empty() {
                    validate_url_text("content-process opener URL", requested_opener_url)?;
                }
                let requested_frame_context = decode_frame_script_context(
                    request.get("frame_context"),
                    "decode content process frame script context",
                )?;
                let requested_environment = request
                    .get("environment")
                    .map(|value| {
                        serde_json::from_value::<NativeEnvironmentOverrides>(value.clone()).map_err(
                            |_| {
                                NativeEngineError::invalid(
                                    "content-process environment",
                                    "must be a valid native environment override",
                                )
                            },
                        )
                    })
                    .transpose()?
                    .unwrap_or_default();
                requested_environment.validate()?;
                let requested_path = request
                    .get("storage_path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
                let requested_file_roots = request
                    .get("allowed_file_roots")
                    .map(|value| {
                        serde_json::from_value::<Vec<PathBuf>>(value.clone()).map_err(|_| {
                            NativeEngineError::invalid(
                                "content-process allowed file roots",
                                "must be an array of path strings",
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or_default();
                let requested_service_worker_clients = request
                    .get("service_worker_clients")
                    .map(|value| {
                        serde_json::from_value::<Vec<NativeServiceWorkerClientState>>(value.clone())
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "content-process Service Worker clients",
                                    "must be a valid native client projection",
                                )
                            })
                    })
                    .transpose()?
                    .unwrap_or_default();
                let loaded_web_storage = requested_path
                    .as_deref()
                    .map(|path| load_web_storage_profile(Some(path)))
                    .transpose();
                let loaded_indexed_db = requested_path
                    .as_deref()
                    .map(|path| load_indexed_db_profile(Some(path)))
                    .transpose();
                let loaded_service_worker_caches = requested_path
                    .as_deref()
                    .map(|path| load_service_worker_cache_profile(Some(path)))
                    .transpose();
                let loaded_service_worker_registrations = requested_path
                    .as_deref()
                    .map(|path| load_service_worker_registration_profiles(Some(path)))
                    .transpose();
                match (
                    loaded_web_storage,
                    loaded_indexed_db,
                    loaded_service_worker_caches,
                    loaded_service_worker_registrations,
                ) {
                    (
                        Ok(loaded),
                        Ok(loaded_indexed_db),
                        Ok(loaded_service_worker_caches),
                        Ok(loaded_service_worker_registrations),
                    ) => {
                        if let Some(loaded) = loaded {
                            storage_state = loaded;
                        }
                        if let Some(loaded) = loaded_indexed_db {
                            indexed_db_state = loaded;
                        }
                        if let Some(loaded) = loaded_service_worker_caches {
                            service_workers.replace_cache_state(loaded);
                        }
                        if let Some(loaded) = loaded_service_worker_registrations {
                            service_workers.replace_registration_profiles(loaded)?;
                        }
                        service_workers.replace_client_states(requested_service_worker_clients)?;
                        storage_profile_path = requested_path;
                        allowed_file_roots = requested_file_roots;
                        storage_context_id = requested_context_id.to_owned();
                        frame_id = requested_frame_id.to_owned();
                        window_name = requested_window_name.to_owned();
                        opener_context_id = requested_opener_context_id.map(str::to_owned);
                        opener_window_name = requested_opener_window_name.to_owned();
                        opener_url = requested_opener_url.to_owned();
                        frame_script_context = requested_frame_context;
                        environment = requested_environment;
                        scroll_offset = NativePoint { x: 0, y: 0 };
                        nested_scroll_offsets.clear();
                        running = true;
                        json!({"kind":"started","id":id})
                    }
                    (Err(error), _, _, _)
                    | (_, Err(error), _, _)
                    | (_, _, Err(error), _)
                    | (_, _, _, Err(error)) => content_error_response(id, error),
                }
            }
            "environment_sync" if protocol_matches(&request) && running => {
                let value = request.get("environment").ok_or_else(|| {
                    NativeEngineError::invalid("content-process environment", "must be present")
                })?;
                let next_environment: NativeEnvironmentOverrides =
                    serde_json::from_value(value.clone()).map_err(|_| {
                        NativeEngineError::invalid(
                            "content-process environment",
                            "must be a valid native environment override",
                        )
                    })?;
                next_environment.validate()?;
                environment = next_environment.clone();
                if let Some(loader) = resource_loader.as_mut() {
                    loader.set_environment(&environment)?;
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_environment(next_environment);
                }
                json!({"kind":"environment_synced","id":id})
            }
            "viewport_sync" if protocol_matches(&request) && running => {
                let width = request
                    .get("width")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process viewport width",
                            "must be a positive bounded integer",
                        )
                    })?;
                let height = request
                    .get("height")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process viewport height",
                            "must be a positive bounded integer",
                        )
                    })?;
                let device_scale_factor_milli = request
                    .get("device_scale_factor_milli")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process viewport device scale factor",
                            "must be a positive bounded integer",
                        )
                    })?;
                let next_viewport = Viewport {
                    width,
                    height,
                    device_scale_factor_milli,
                };
                next_viewport.validate()?;
                viewport = next_viewport;
                if let Some(document) = document.as_mut() {
                    document.set_viewport(next_viewport)?;
                }
                json!({"kind":"viewport_synced","id":id})
            }
            "document_sync" if protocol_matches(&request) && running => {
                let current_generation = document.as_ref().map(NativeDocument::generation);
                let requested_generation = request
                    .get("generation")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok());
                let prepared = (|| {
                    let Some(current_generation) = current_generation else {
                        return Err(NativeEngineError::Worker {
                            operation: "content process document synchronization".into(),
                            reason: "content process has no committed document".into(),
                        });
                    };
                    let Some(requested_generation) = requested_generation else {
                        return Err(NativeEngineError::invalid(
                            "content-process document generation",
                            "must be a positive integer",
                        ));
                    };
                    if requested_generation != current_generation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "content process document generation is stale".into(),
                        });
                    }
                    let limits: NativeEngineLimits = request
                        .get("limits")
                        .cloned()
                        .ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process document limits",
                                "must be present",
                            )
                        })
                        .and_then(|value| {
                            serde_json::from_value(value).map_err(|_| {
                                NativeEngineError::invalid(
                                    "content-process document limits",
                                    "must be valid native engine limits",
                                )
                            })
                        })?;
                    limits.validate()?;
                    let wire: NativeDocumentWire = request
                        .get("document")
                        .cloned()
                        .ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process document snapshot",
                                "must be present",
                            )
                        })
                        .and_then(|value| {
                            serde_json::from_value(value).map_err(|_| {
                                NativeEngineError::invalid(
                                    "content-process document snapshot",
                                    "must be a valid bounded native document",
                                )
                            })
                        })?;
                    let mut next =
                        NativeDocument::from_content_wire(wire, &limits, requested_generation)?;
                    if let Some(url) = document_url.as_deref() {
                        next.set_css_target_from_url(url)?;
                    }
                    Ok(next)
                })();
                match prepared {
                    Ok(next) => {
                        document = Some(next);
                        json!({"kind":"document_synced","id":id})
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "commit" if protocol_matches(&request) && running => {
                json!({"kind":"committed","id":id})
            }
            "scroll_sync" if protocol_matches(&request) && running => {
                if request.get("x").is_some() || request.get("y").is_some() {
                    let x = request
                        .get("x")
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process scroll x",
                                "must be a non-negative integer",
                            )
                        })?;
                    let y = request
                        .get("y")
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process scroll y",
                                "must be a non-negative integer",
                            )
                        })?;
                    scroll_offset = NativePoint { x, y };
                }
                if let Some(value) = request.get("nested") {
                    let entries = value.as_array().ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process nested scroll offsets",
                            "must be an array",
                        )
                    })?;
                    if entries.len() > MAX_NATIVE_NODES {
                        return Err(NativeEngineError::limit(
                            "content-process nested scroll offsets",
                            MAX_NATIVE_NODES,
                            entries.len(),
                        ));
                    }
                    let mut next_offsets = BTreeMap::new();
                    for entry in entries {
                        let node_index = entry
                            .get("node_index")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process nested scroll node",
                                    "must be a non-negative integer",
                                )
                            })?;
                        let x = entry
                            .get("x")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process nested scroll x",
                                    "must be a non-negative integer",
                                )
                            })?;
                        let y = entry
                            .get("y")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process nested scroll y",
                                    "must be a non-negative integer",
                                )
                            })?;
                        next_offsets.insert(node_index, NativePoint { x, y });
                    }
                    nested_scroll_offsets = next_offsets;
                }
                if request.get("x").is_none()
                    && request.get("y").is_none()
                    && request.get("nested").is_none()
                {
                    return Err(NativeEngineError::invalid(
                        "content-process scroll synchronization",
                        "must contain a root or nested scroll state",
                    ));
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_scroll_offset(scroll_offset);
                    runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                }
                json!({"kind":"scroll_synced","id":id})
            }
            "history_sync" if protocol_matches(&request) && running => {
                let requested_url =
                    request.get("url").and_then(Value::as_str).ok_or_else(|| {
                        NativeEngineError::invalid("content-process history URL", "must be text")
                    })?;
                validate_url_text("content-process history URL", requested_url)?;
                let requested_state = request.get("state").ok_or_else(|| {
                    NativeEngineError::invalid("content-process history state", "must be present")
                })?;
                validate_content_history_state(requested_state)?;
                let length = request
                    .get("length")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process history length",
                            "must be a positive integer",
                        )
                    })?;
                if length == 0 {
                    return Err(NativeEngineError::invalid(
                        "content-process history length",
                        "must be a positive integer",
                    ));
                }
                document_url = Some(requested_url.to_owned());
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_history_state(requested_state.clone());
                    runtime.set_history_length(length);
                }
                json!({"kind":"history_synced","id":id})
            }
            "document_cookie_projection" if protocol_matches(&request) && running => {
                let owner = decode_content_cookie_owner(
                    request.get("owner").ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process document.cookie projection owner",
                            "must be present",
                        )
                    })?,
                    "content-process document.cookie projection owner",
                )?;
                let active_generation = document.as_ref().map(NativeDocument::generation);
                if owner.context_id != storage_context_id
                    || owner.frame_id != frame_id
                    || Some(owner.generation) != active_generation
                    || document_url.as_deref().is_none_or(|active_url| {
                        without_fragment(active_url) != without_fragment(&owner.document_url)
                    })
                {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason:
                            "document.cookie projection belongs to a stale or different document"
                                .into(),
                    });
                }
                let projection = request
                    .get("document_cookie")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process document.cookie projection",
                            "must be text",
                        )
                    })?;
                if projection.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
                    return Err(NativeEngineError::limit(
                        "content-process document.cookie projection",
                        MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                        projection.len(),
                    ));
                }
                if projection.chars().any(char::is_control) {
                    return Err(NativeEngineError::invalid(
                        "content-process document.cookie projection",
                        "must not contain control characters",
                    ));
                }
                let Some(runtime) = javascript_runtime.as_ref() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process document.cookie projection".into(),
                        reason: "content process has no active JavaScript runtime".into(),
                    });
                };
                parent_document_cookie_projection = Some(projection.to_owned());
                runtime.set_cookie_state(projection.to_owned());
                json!({"kind":"document_cookie_projection_synced","id":id})
            }
            "storage_events" if protocol_matches(&request) && running => {
                let values = request.get("events").ok_or_else(|| {
                    NativeEngineError::invalid("content-process storage events", "must be an array")
                })?;
                let events =
                    decode_storage_event_value(values, "decode content process storage events")?;
                for event in &events {
                    storage_state.apply_storage_event(event)?;
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_storage_state(storage_state.clone());
                    runtime.set_storage_events(events)?;
                }
                json!({"kind":"storage_events_synced","id":id})
            }
            "window_proxy_sync" if protocol_matches(&request) && running => {
                let updates = decode_window_proxy_updates(
                    request.get("updates"),
                    "decode content process WindowProxy updates",
                )?;
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.sync_window_proxies(&updates)?;
                }
                json!({"kind":"window_proxy_synced","id":id})
            }
            "frame_script_sync" if protocol_matches(&request) && running => {
                let bindings = decode_frame_script_bindings(
                    request.get("bindings"),
                    "decode content process frame script bindings",
                )?;
                frame_script_bindings = bindings;
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_frame_script_bindings(frame_script_bindings.clone());
                }
                json!({"kind":"frame_script_synced","id":id})
            }
            "frame_script_context_sync" if protocol_matches(&request) && running => {
                frame_script_context = decode_frame_script_context(
                    request.get("context"),
                    "decode content process frame script context",
                )?;
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_frame_script_context(frame_script_context.clone());
                }
                json!({"kind":"frame_script_context_synced","id":id})
            }
            "service_worker_clients_sync" if protocol_matches(&request) && running => {
                let value = request.get("clients").ok_or_else(|| {
                    NativeEngineError::invalid(
                        "content-process Service Worker clients",
                        "must be present",
                    )
                })?;
                let clients: Vec<NativeServiceWorkerClientState> =
                    serde_json::from_value(value.clone()).map_err(|_| {
                        NativeEngineError::invalid(
                            "content-process Service Worker clients",
                            "must be a valid native client projection",
                        )
                    })?;
                service_workers.replace_client_states(clients)?;
                json!({"kind":"service_worker_clients_synced","id":id})
            }
            "service_worker_registrations_sync" if protocol_matches(&request) && running => {
                if let Some(path) = storage_profile_path.as_deref() {
                    let profiles = load_service_worker_registration_profiles(Some(path))?;
                    service_workers.replace_registration_profiles(profiles)?;
                    if let (Some(runtime), Some(document_url)) =
                        (javascript_runtime.as_ref(), document_url.as_deref())
                    {
                        runtime.set_service_worker_registrations(
                            service_workers.states_for_document(document_url)?,
                        );
                    }
                }
                json!({"kind":"service_worker_registrations_synced","id":id})
            }
            "service_worker_open_window_resolve" if protocol_matches(&request) && running => {
                let worker_id = request
                    .get("worker_id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process service worker openWindow worker id",
                            "must be a positive integer",
                        )
                    })?;
                let request_id = request
                    .get("request_id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process service worker openWindow request id",
                            "must be a positive integer",
                        )
                    })?;
                let window = request.get("window").ok_or_else(|| {
                    NativeEngineError::invalid(
                        "content-process service worker openWindow response",
                        "must be present",
                    )
                })?;
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process service worker openWindow resolution".into(),
                        reason: "content process has no resource loader".into(),
                    });
                };
                let active_owner = match (document.as_ref(), document_url.as_deref()) {
                    (Some(document), Some(document_url)) => {
                        Some((document.generation(), document_url))
                    }
                    _ => pending_service_worker_navigation
                        .as_ref()
                        .and_then(|pending| {
                            Some((
                                pending.get("generation")?.as_u64()?.try_into().ok()?,
                                pending.get("url")?.as_str()?,
                            ))
                        }),
                };
                let owner = decode_content_cookie_owner(
                    request.get("owner").ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process service worker openWindow owner",
                            "must be present",
                        )
                    })?,
                    "content-process service worker openWindow owner",
                )?;
                let owner_matches = active_owner.is_some_and(|(generation, url)| {
                    owner.generation == generation
                        && without_fragment(&owner.document_url) == without_fragment(url)
                });
                if owner.context_id != storage_context_id
                    || owner.frame_id != frame_id
                    || !owner_matches
                {
                    return Err(NativeEngineError::worker_failure(
                        "content process service worker openWindow owner",
                        NativeWorkerFailureKind::Protocol,
                        "parent owner does not match the active content document",
                    ));
                }
                let runtime = javascript_runtime.as_ref();
                let ipc_request_id = id.as_u64().ok_or_else(|| {
                    NativeEngineError::invalid(
                        "content-process service worker openWindow request ID",
                        "must be an unsigned integer",
                    )
                })?;
                let mut parent_fetch_broker = NativeContentFetchBroker {
                    captured_load_fetches: false,
                    request_id: ipc_request_id,
                    owner: owner.clone(),
                    runtime,
                    stdout: &mut stdout,
                    ipc_requests: &mut ipc_request_rx,
                    cancelled_parent_fetches: &mut cancelled_parent_fetches,
                    document_cookie_projection: &mut parent_document_cookie_projection,
                    next_content_resource_fetch_id: 0,
                    page_meta_content_security_policies: loader
                        .document_meta_content_security_policies(&owner.document_url)?,
                };
                match service_workers
                    .resolve_open_window(
                        loader,
                        worker_id,
                        request_id,
                        window,
                        &mut parent_fetch_broker,
                    )
                    .await
                {
                    Ok(service_worker_fetch_resumed) => {
                        pending_message_port_messages
                            .extend(service_workers.take_message_port_messages());
                        pending_service_worker_client_messages
                            .extend(service_workers.take_client_messages());
                        json!({
                            "kind": "evaluated",
                            "id": id,
                            "value": Value::Null,
                            "history": [],
                            "frame_scripts": [],
                            "service_worker_fetch_resumed": service_worker_fetch_resumed,
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "storage_state" if protocol_matches(&request) && running => {
                let value = request.get("state").ok_or_else(|| {
                    NativeEngineError::invalid("content-process storage state", "must be an object")
                })?;
                let next_state: NativeWebStorageState = serde_json::from_value(value.clone())
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "content-process storage state",
                            "must be a valid native Web Storage state",
                        )
                    })?;
                next_state.validate()?;
                storage_state = next_state;
                if let Some(value) = request.get("indexed_db_state") {
                    let next_indexed_db_state: NativeIndexedDbState =
                        serde_json::from_value(value.clone()).map_err(|_| {
                            NativeEngineError::invalid(
                                "content-process IndexedDB state",
                                "must be a valid native IndexedDB state",
                            )
                        })?;
                    next_indexed_db_state.validate()?;
                    indexed_db_state = next_indexed_db_state;
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.replace_storage_state(storage_state.clone());
                    if let (Some(document_url), Some(document_origin)) =
                        (document_url.as_deref(), document_origin.as_ref())
                    {
                        runtime.set_indexed_db_state(
                            indexed_db_state.origin(&storage_key(document_url, document_origin)),
                        );
                    }
                }
                json!({"kind":"storage_state_synced","id":id})
            }
            "load" if protocol_matches(&request) && running => {
                let resume_service_worker_fetch = request
                    .get("resume_service_worker_fetch")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !resume_service_worker_fetch && pending_service_worker_navigation.is_some() {
                    content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process load".into(),
                            reason:
                                "a Service Worker fetch navigation is awaiting browser resolution"
                                    .into(),
                        },
                    )
                } else {
                    if !resume_service_worker_fetch {
                        parent_document_cookie_projection = None;
                        frame_script_bindings.clear();
                        websocket_connections.clear();
                        worker_websocket_connections.clear();
                        pending_worker_websocket_commands.clear();
                        fetch_stream_connections.clear();
                        for (_, pending) in std::mem::take(&mut pending_upload_fetches) {
                            pending.task.abort();
                        }
                        for (_, pending) in std::mem::take(&mut pending_controlled_uploads) {
                            pending.task.abort();
                        }
                        for (_, connection) in std::mem::take(&mut fetch_upload_connections) {
                            let _ = connection
                                .commands
                                .try_send(NativeFetchUploadCommand::Cancel);
                        }
                        event_source_connections.clear();
                        worker_event_source_connections.clear();
                        workers.clear();
                        pending_worker_messages.clear();
                        pending_message_port_messages.clear();
                        pending_service_worker_client_messages.clear();
                        pending_external_service_worker_client_messages.clear();
                        pending_page_message_port_commands.clear();
                        pending_shared_worker_commands.clear();
                        pending_service_worker_open_windows.clear();
                        service_workers.clear_page_message_port_routes();
                    }
                    let load_request = if resume_service_worker_fetch {
                        pending_service_worker_navigation.clone().ok_or_else(|| {
                            NativeEngineError::Worker {
                                operation: "content process service worker fetch resume".into(),
                                reason: "no suspended navigation is available".into(),
                            }
                        })?
                    } else {
                        request.clone()
                    };
                    let resumed_fetch = if resume_service_worker_fetch {
                        Some(service_workers.take_completed_fetch().ok_or_else(|| {
                            NativeEngineError::Worker {
                                operation: "content process service worker fetch resume".into(),
                                reason: "the suspended fetch has no completed response".into(),
                            }
                        })?)
                    } else {
                        None
                    };
                    if let Some(runtime) = javascript_runtime.as_ref() {
                        storage_state = runtime.storage_state();
                    }
                    match load_content_resource(
                        request_id,
                        &load_request,
                        &mut resource_loader,
                        storage_profile_path.as_deref(),
                        &allowed_file_roots,
                        &environment,
                        &mut service_workers,
                        resumed_fetch,
                        &storage_context_id,
                        &frame_id,
                        &mut stdout,
                        &mut ipc_request_rx,
                        &mut cancelled_parent_fetches,
                        &mut parent_document_cookie_projection,
                    )
                    .await
                    {
                        Ok(Some((
                            resource,
                            mut parsed,
                            loaded_viewport,
                            script_sources,
                            resource_events,
                        ))) => {
                            if resume_service_worker_fetch {
                                pending_service_worker_navigation = None;
                            }
                            pending_service_worker_client_messages
                                .extend(service_workers.take_client_messages());
                            let mut script_runtime =
                                match NativeJavaScriptRuntime::new_with_context_metadata_and_dialog_handler(
                                    &storage_context_id,
                                    &window_name,
                                    opener_context_id.as_deref(),
                                    &opener_window_name,
                                    &opener_url,
                                    Arc::clone(&dialog_handler),
                                ) {
                                    Ok(runtime) => Some(runtime),
                                    Err(error) => {
                                        let response = content_error_response(id, error);
                                        write_value_frame(&mut stdout, &response).await?;
                                        continue;
                                    }
                                };
                            if let Some(runtime) = script_runtime.as_ref() {
                                runtime.set_environment(environment.clone());
                                runtime.set_frame_id(frame_id.clone());
                                runtime.set_frame_script_context(frame_script_context.clone());
                                runtime.set_frame_script_bindings(frame_script_bindings.clone());
                                if let Ok(registrations) =
                                    service_workers.states_for_document(&resource.url)
                                {
                                    runtime.set_service_worker_registrations(registrations);
                                }
                                runtime.set_scroll_offset(scroll_offset);
                                runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                                if let Some(loader) = resource_loader.as_ref() {
                                    runtime.set_inline_script_policy(
                                        loader.inline_script_policy(&resource.url)?,
                                    );
                                }
                            }
                            if let (Some(runtime), Some(loader)) =
                                (script_runtime.as_ref(), resource_loader.as_ref())
                            {
                                runtime.set_sync_xhr_loader(loader);
                            }
                            let document_cookie = match parent_document_cookie_projection.as_ref() {
                                Some(projection) => projection.clone(),
                                None => resource_loader
                                    .as_ref()
                                    .map(|loader| loader.document_cookie(&resource.url))
                                    .transpose()?
                                    .unwrap_or_default(),
                            };
                            let mut page_scripts = execute_page_scripts(
                                &mut parsed,
                                &mut script_runtime,
                                &storage_context_id,
                                &script_sources,
                                &resource.url,
                                &resource.origin,
                                loaded_viewport,
                                &storage_state,
                                &indexed_db_state,
                                &document_cookie,
                                &resource_events,
                                &resource.csp_violations,
                            );
                            if page_scripts.is_ok()
                                && let Some(loader) = resource_loader.as_mut()
                                && let Err(error) = apply_pending_meta_content_security_policies(
                                    &mut parsed,
                                    loader,
                                    &resource.url,
                                )
                            {
                                page_scripts = Err(error);
                            }
                            let initial_dynamic_sources = page_scripts
                                .as_mut()
                                .map(|page_scripts| {
                                    std::mem::take(&mut page_scripts.pending_script_sources)
                                })
                                .unwrap_or_default();
                            if !initial_dynamic_sources.is_empty() {
                                let dynamic_result =
                                    match (script_runtime.as_ref(), resource_loader.as_mut()) {
                                        (Some(runtime), Some(loader)) => {
                                            let mut parent_fetch_broker =
                                                NativeContentFetchBroker {
                                                    captured_load_fetches: true,
                                                    request_id: id.as_u64().ok_or_else(|| {
                                                        NativeEngineError::invalid(
                                                            "content-process load request ID",
                                                            "must be an unsigned integer",
                                                        )
                                                    })?,
                                                    owner: NativeContentCookieOwner {
                                                        context_id: storage_context_id.clone(),
                                                        frame_id: frame_id.clone(),
                                                        generation: parsed.generation(),
                                                        document_url: resource.url.clone(),
                                                    },
                                                    runtime: Some(runtime),
                                                    stdout: &mut stdout,
                                                    ipc_requests: &mut ipc_request_rx,
                                                    cancelled_parent_fetches:
                                                        &mut cancelled_parent_fetches,
                                                    document_cookie_projection:
                                                        &mut parent_document_cookie_projection,
                                                    next_content_resource_fetch_id: 0,
                                                    page_meta_content_security_policies: loader
                                                        .document_meta_content_security_policies(
                                                            &resource.url,
                                                        )?,
                                                };
                                            execute_dynamic_page_scripts_with_loader(
                                                &mut parsed,
                                                runtime,
                                                initial_dynamic_sources,
                                                loader,
                                                &resource.url,
                                                &resource.origin,
                                                loaded_viewport,
                                                Some(&mut parent_fetch_broker),
                                            )
                                            .await
                                        }
                                        _ => Err(NativeEngineError::Worker {
                                            operation: "initial dynamic page script".into(),
                                            reason:
                                                "content process script loader state is unavailable"
                                                    .into(),
                                        }),
                                    };
                                match dynamic_result {
                                    Ok(dynamic_result) => {
                                        if let Ok(page_scripts) = page_scripts.as_mut() {
                                            merge_dynamic_page_script_result(
                                                page_scripts,
                                                dynamic_result,
                                            )?;
                                        }
                                    }
                                    Err(error) => page_scripts = Err(error),
                                }
                            }
                            let mut post_script_csp_result = NativePageScriptResult::default();
                            let mut post_script_csp_events = Vec::new();
                            let mut post_script_csp_history = Vec::new();
                            let mut post_script_csp_scroll_commands = Vec::new();
                            let mut post_script_document_url = resource.url.clone();
                            if let (Some(loader), Some(runtime)) =
                                (resource_loader.as_mut(), script_runtime.as_ref())
                            {
                                refresh_inline_style_policy(&mut parsed, loader, &resource.url)?;
                                dispatch_pending_csp_violations(
                                    &mut parsed,
                                    runtime,
                                    &mut post_script_document_url,
                                    &resource.origin,
                                    loaded_viewport,
                                    loader,
                                    &mut post_script_csp_result,
                                    &mut post_script_csp_events,
                                    &mut post_script_csp_history,
                                    &mut post_script_csp_scroll_commands,
                                )?;
                            }
                            drop(post_script_csp_history);
                            drop(post_script_document_url);
                            if let Ok(page_scripts) = page_scripts.as_mut() {
                                merge_dynamic_page_script_result(
                                    page_scripts,
                                    post_script_csp_result,
                                )?;
                                page_scripts.events.extend(
                                    post_script_csp_events
                                        .into_iter()
                                        .map(|(node_index, kind)| (node_index.index(), kind)),
                                );
                                page_scripts
                                    .scroll_commands
                                    .extend(post_script_csp_scroll_commands);
                            }
                            let prepared = match page_scripts {
                                Ok(page_scripts) if page_scripts.navigation.is_some() => {
                                    let dialogs = page_scripts.dialogs;
                                    let events = page_scripts.events;
                                    let scroll_commands = page_scripts.scroll_commands;
                                    let navigation = page_scripts.navigation.map(|navigation| {
                                        NativeContentNavigation {
                                            node_index: 0,
                                            href: navigation.href,
                                            submitter_node_index: None,
                                            target: "_self".into(),
                                            method: NativeNavigationMethod::Get,
                                            body: None,
                                            body_content_type: None,
                                            location: true,
                                            replace_history: navigation.replace_history,
                                            object_url: navigation.object_url,
                                        }
                                    });
                                    Ok((parsed, navigation, dialogs, events, scroll_commands))
                                }
                                Ok(page_scripts)
                                    if page_scripts.pending_fetches.is_empty()
                                        && page_scripts.service_worker_commands.is_empty()
                                        && page_scripts.websocket_commands.is_empty()
                                        && page_scripts.event_source_commands.is_empty() =>
                                {
                                    Ok((
                                        parsed,
                                        None,
                                        page_scripts.dialogs,
                                        page_scripts.events,
                                        page_scripts.scroll_commands,
                                    ))
                                }
                                Ok(mut page_scripts) => {
                                    let page_dialogs = page_scripts.dialogs;
                                    let mut page_events = page_scripts.events;
                                    let mut page_scroll_commands = page_scripts.scroll_commands;
                                    let service_worker_commands =
                                        std::mem::take(&mut page_scripts.service_worker_commands);
                                    if !service_worker_commands.is_empty() {
                                        let resolved = match (
                                            script_runtime.as_ref(),
                                            resource_loader.as_mut(),
                                        ) {
                                            (Some(runtime), Some(loader)) => {
                                                let mut parent_fetch_broker =
                                                    NativeContentFetchBroker {
                                                        captured_load_fetches: true,
                                                        request_id: id.as_u64().ok_or_else(|| {
                                                            NativeEngineError::invalid(
                                                                "content-process load request ID",
                                                                "must be an unsigned integer",
                                                            )
                                                        })?,
                                                        owner: NativeContentCookieOwner {
                                                            context_id:
                                                                storage_context_id.clone(),
                                                            frame_id: frame_id.clone(),
                                                            generation: parsed.generation(),
                                                            document_url: resource.url.clone(),
                                                        },
                                                        runtime: Some(runtime),
                                                        stdout: &mut stdout,
                                                        ipc_requests: &mut ipc_request_rx,
                                                        cancelled_parent_fetches: &mut cancelled_parent_fetches,
                                                        document_cookie_projection:
                                                            &mut parent_document_cookie_projection,
                                                        next_content_resource_fetch_id: 0,
                                                        page_meta_content_security_policies: loader
                                                            .document_meta_content_security_policies(
                                                                &resource.url,
                                                            )?,
                                                    };
                                                resolve_service_worker_commands(
                                                    service_worker_commands,
                                                    &mut service_workers,
                                                    loader,
                                                    runtime,
                                                    &mut parsed,
                                                    &resource.url,
                                                    &resource.origin,
                                                    loaded_viewport,
                                                    Some(&mut parent_fetch_broker),
                                                )
                                                .await?
                                            }
                                            _ => {
                                                return Err(NativeEngineError::Worker {
                                                operation: "service worker registration".into(),
                                                reason:
                                                    "content process service worker state is unavailable"
                                                        .into(),
                                            });
                                            }
                                        };
                                        pending_message_port_messages
                                            .extend(service_workers.take_message_port_messages());
                                        pending_service_worker_client_messages
                                            .extend(service_workers.take_client_messages());
                                        page_scripts.pending_fetches.extend(resolved);
                                    }
                                    match (script_runtime.as_ref(), resource_loader.as_mut()) {
                                        (Some(runtime), Some(loader)) => {
                                            let parent_fetch_broker = NativeContentFetchBroker {
                                                captured_load_fetches: true,
                                                request_id: id.as_u64().ok_or_else(|| {
                                                    NativeEngineError::invalid(
                                                        "content-process load request ID",
                                                        "must be an unsigned integer",
                                                    )
                                                })?,
                                                owner: NativeContentCookieOwner {
                                                    context_id: storage_context_id.clone(),
                                                    frame_id: frame_id.clone(),
                                                    generation: parsed.generation(),
                                                    document_url: resource.url.clone(),
                                                },
                                                runtime: Some(runtime),
                                                stdout: &mut stdout,
                                                ipc_requests: &mut ipc_request_rx,
                                                cancelled_parent_fetches:
                                                    &mut cancelled_parent_fetches,
                                                document_cookie_projection:
                                                    &mut parent_document_cookie_projection,
                                                next_content_resource_fetch_id: 0,
                                                page_meta_content_security_policies: loader
                                                    .document_meta_content_security_policies(
                                                        &resource.url,
                                                    )?,
                                            };
                                            let script_fetch_result = resolve_script_fetches(
                                                &parsed,
                                                runtime,
                                                Some(loader),
                                                Some(parent_fetch_broker),
                                                &mut service_workers,
                                                &mut websocket_connections,
                                                &mut fetch_stream_connections,
                                                &mut fetch_upload_connections,
                                                &mut event_source_connections,
                                                &resource.url,
                                                &resource.origin,
                                                loaded_viewport,
                                                false,
                                                NativeScriptEvaluation {
                                                    value: Value::Null,
                                                    commands: page_scripts
                                                        .pending_fetches
                                                        .into_iter()
                                                        .chain(page_scripts.websocket_commands)
                                                        .chain(page_scripts.event_source_commands)
                                                        .collect(),
                                                    top_level_await_pending: false,
                                                    worker_script_error: None,
                                                },
                                            )
                                            .await;
                                            pending_message_port_messages.extend(
                                                service_workers.take_message_port_messages(),
                                            );
                                            pending_service_worker_client_messages
                                                .extend(service_workers.take_client_messages());
                                            match script_fetch_result {
                                                Ok((next, mutation, _resolved_value)) => {
                                                    page_events.extend(
                                                        mutation.events.into_iter().map(|event| {
                                                            (event.node_index, event.kind)
                                                        }),
                                                    );
                                                    page_scroll_commands
                                                        .extend(mutation.scroll_commands);
                                                    Ok((
                                                        next,
                                                        mutation.navigation,
                                                        page_dialogs,
                                                        page_events,
                                                        page_scroll_commands,
                                                    ))
                                                }
                                                Err(error) => Err(error),
                                            }
                                        }
                                        _ => Err(NativeEngineError::Worker {
                                            operation: "page-load fetch scheduling".into(),
                                            reason: "content process fetch state is unavailable"
                                                .into(),
                                        }),
                                    }
                                }
                                Err(error) => Err(error),
                            };
                            match prepared {
                                Ok((parsed, navigation, dialogs, events, scroll_commands)) => {
                                    if let Some(runtime) = script_runtime.as_ref() {
                                        storage_state = runtime.storage_state();
                                        indexed_db_state.replace_origin(
                                            storage_key(&resource.url, &resource.origin),
                                            runtime.indexed_db_state(),
                                        )?;
                                        let Some(loader) = resource_loader.as_mut() else {
                                            return Err(NativeEngineError::Worker {
                                                operation: "page-load Worker scheduling".into(),
                                                reason:
                                                    "content process resource loader is unavailable"
                                                        .into(),
                                            });
                                        };
                                        let worker_commands = defer_shared_worker_commands(
                                            runtime.take_worker_commands(),
                                            external_shared_worker_routing,
                                            &resource.origin,
                                            &storage_context_id,
                                            &frame_id,
                                            parsed.generation(),
                                            &mut pending_shared_worker_commands,
                                        )?;
                                        let message_port_commands =
                                            runtime.take_message_port_commands();
                                        service_workers
                                            .apply_page_message_port_commands(
                                                message_port_commands.clone(),
                                                loader,
                                                None,
                                            )
                                            .await?;
                                        let mut parent_fetch_broker = NativeContentFetchBroker {
                                            captured_load_fetches: true,
                                            request_id: id.as_u64().ok_or_else(|| {
                                                NativeEngineError::invalid(
                                                    "content-process load request ID",
                                                    "must be an unsigned integer",
                                                )
                                            })?,
                                            owner: NativeContentCookieOwner {
                                                context_id: storage_context_id.clone(),
                                                frame_id: frame_id.clone(),
                                                generation: parsed.generation(),
                                                document_url: resource.url.clone(),
                                            },
                                            runtime: Some(runtime),
                                            stdout: &mut stdout,
                                            ipc_requests: &mut ipc_request_rx,
                                            cancelled_parent_fetches: &mut cancelled_parent_fetches,
                                            document_cookie_projection:
                                                &mut parent_document_cookie_projection,
                                            next_content_resource_fetch_id: 0,
                                            page_meta_content_security_policies: loader
                                                .document_meta_content_security_policies(
                                                    &resource.url,
                                                )?,
                                        };
                                        workers
                                            .apply_commands_with_parent_fetch_broker(
                                                worker_commands,
                                                loader,
                                                &resource.url,
                                                &mut parent_fetch_broker,
                                            )
                                            .await?;
                                        apply_worker_page_message_port_commands(
                                            &mut workers,
                                            message_port_commands,
                                            loader,
                                            external_shared_worker_routing,
                                            Some(&mut parent_fetch_broker),
                                        )
                                        .await?;
                                        let mut worker_websocket_commands =
                                            pending_worker_websocket_commands
                                                .drain(..)
                                                .collect::<Vec<_>>();
                                        worker_websocket_commands
                                            .extend(workers.take_websocket_commands());
                                        process_worker_websocket_commands(
                                            worker_websocket_commands,
                                            &mut worker_websocket_connections,
                                            Some(&mut parent_fetch_broker),
                                        )
                                        .await?;
                                        pump_worker_websocket_event(
                                            &mut workers,
                                            &mut worker_websocket_connections,
                                            loader,
                                            Some(&mut parent_fetch_broker),
                                            &mut worker_websocket_read_cursor,
                                        )
                                        .await?;
                                        process_worker_event_source_commands(
                                            workers.take_event_source_commands(),
                                            &mut worker_event_source_connections,
                                            Some(&mut parent_fetch_broker),
                                        )
                                        .await?;
                                        let _ = pump_parent_worker_event_source_stream(
                                            &mut worker_event_source_connections,
                                            Some(&mut parent_fetch_broker),
                                            &mut worker_event_source_read_cursor,
                                        )
                                        .await?;
                                        pump_worker_event_source_event(
                                            &mut workers,
                                            &mut worker_event_source_connections,
                                            loader,
                                            Some(&mut parent_fetch_broker),
                                        )
                                        .await?;
                                        pending_worker_messages.extend(workers.take_messages());
                                        pending_message_port_messages
                                            .extend(workers.take_message_port_messages());
                                        pending_page_message_port_commands
                                            .extend(workers.take_page_message_port_commands());
                                    }
                                    let generation = parsed.generation();
                                    let document_wire = parsed.to_content_wire();
                                    document = Some(parsed);
                                    document_url = Some(resource.url.clone());
                                    document_origin = Some(resource.origin.clone());
                                    viewport = loaded_viewport;
                                    javascript_runtime = script_runtime;
                                    json!({
                                        "kind": "loaded",
                                        "id": id,
                                        "generation": generation,
                                        "url": resource.url,
                                        "navigation": navigation.as_ref().map(content_navigation_json),
                                        "dialogs": dialogs,
                                        "events": events.iter().map(|(node_index, kind)| json!({
                                            "node_index": node_index,
                                            "kind": event_kind_text(*kind),
                                        })).collect::<Vec<_>>(),
                                        "scroll_commands": scroll_commands,
                                        "frame_sources": resource.frame_sources,
                                        "navigate_to_sources": resource.navigate_to_sources,
                                        "document_base64": base64::engine::general_purpose::STANDARD
                                            .encode(serde_json::to_vec(&document_wire).unwrap_or_default()),
                                    })
                                }
                                Err(error) => content_error_response(id, error),
                            }
                        }
                        Ok(None) => {
                            pending_service_worker_navigation = Some(load_request);
                            let open_windows = service_workers.take_open_windows();
                            json!({
                                "kind": "service_worker_fetch_suspended",
                                "id": id,
                                "service_worker_open_windows": open_windows,
                            })
                        }
                        Err(error) => content_error_response(id, error),
                    }
                }
            }
            "fetch" if protocol_matches(&request) && running => {
                let request_document_url = request
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process fetch owner URL",
                            "must be text",
                        )
                    })?;
                let href = request.get("href").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process fetch URL", "must be text")
                })?;
                let credentials = request
                    .get("credentials")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let owner_document =
                    document.as_ref().ok_or_else(|| NativeEngineError::Worker {
                        operation: "content process fetch".into(),
                        reason: "content process has no active document owner".into(),
                    })?;
                let active_owner_url =
                    document_url
                        .as_deref()
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "content process has no active document URL".into(),
                        })?;
                let owner = decode_content_cookie_owner(
                    request.get("owner").ok_or_else(|| {
                        NativeEngineError::invalid("content-process fetch owner", "must be present")
                    })?,
                    "content-process fetch owner",
                )?;
                if owner.context_id != storage_context_id
                    || owner.frame_id != frame_id
                    || owner.generation != owner_document.generation()
                    || without_fragment(&owner.document_url) != without_fragment(active_owner_url)
                    || without_fragment(&owner.document_url)
                        != without_fragment(request_document_url)
                {
                    return Err(NativeEngineError::worker_failure(
                        "content process fetch owner",
                        NativeWorkerFailureKind::Protocol,
                        "parent owner does not match the active content document",
                    ));
                }
                let runtime =
                    javascript_runtime
                        .as_ref()
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "content process has no active JavaScript runtime".into(),
                        })?;
                let ipc_request_id = id.as_u64().ok_or_else(|| {
                    NativeEngineError::invalid(
                        "content-process fetch request ID",
                        "must be an unsigned integer",
                    )
                })?;
                let loader = resource_loader
                    .as_mut()
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "content process fetch".into(),
                        reason: "content process has no resource loader".into(),
                    })?;
                let mut parent_fetch_broker = NativeContentFetchBroker {
                    captured_load_fetches: false,
                    request_id: ipc_request_id,
                    owner: owner.clone(),
                    runtime: Some(runtime),
                    stdout: &mut stdout,
                    ipc_requests: &mut ipc_request_rx,
                    cancelled_parent_fetches: &mut cancelled_parent_fetches,
                    document_cookie_projection: &mut parent_document_cookie_projection,
                    next_content_resource_fetch_id: 0,
                    page_meta_content_security_policies: loader
                        .document_meta_content_security_policies(&owner.document_url)?,
                };
                let intercepted = service_workers
                    .intercept_fetch(
                        loader,
                        request_document_url,
                        href,
                        "GET",
                        BTreeMap::new(),
                        None,
                        None,
                        NativeCorsMode::Cors,
                        NativeFetchRedirectMode::Follow,
                        None,
                        credentials,
                        None,
                        None,
                        None,
                        None,
                        "fetch",
                        None,
                        None,
                        Some(&mut parent_fetch_broker),
                        false,
                    )
                    .await;
                pending_message_port_messages.extend(service_workers.take_message_port_messages());
                pending_service_worker_client_messages
                    .extend(service_workers.take_client_messages());
                let fetch = match intercepted {
                    Ok(NativeServiceWorkerFetchOutcome::Handled(response)) => Ok(response),
                    Ok(NativeServiceWorkerFetchOutcome::NotHandled) => {
                        write_value_frame(
                            &mut stdout,
                            &json!({
                                "kind": "parent_fetch_request",
                                "id": ipc_request_id,
                                "document_url": request_document_url,
                                "href": href,
                                "credentials": credentials,
                            }),
                        )
                        .await?;
                        let payload = receive_content_ipc_response(
                            &mut ipc_request_rx,
                            &mut cancelled_parent_fetches,
                            "parent Fetch broker",
                        )
                        .await?;
                        let broker_response: Value =
                            serde_json::from_slice(&payload).map_err(|_| {
                                NativeEngineError::Worker {
                                    operation: "decode parent Fetch response".into(),
                                    reason: "parent returned invalid broker JSON".into(),
                                }
                            })?;
                        if broker_response.get("kind").and_then(Value::as_str) == Some("error") {
                            Err(NativeEngineError::Network {
                                operation: "parent-brokered fetch".into(),
                                reason: broker_response
                                    .get("reason")
                                    .and_then(Value::as_str)
                                    .unwrap_or("parent rejected the network request")
                                    .to_owned(),
                            })
                        } else {
                            decode_fetch_response(&broker_response, ipc_request_id)
                        }
                    }
                    Ok(NativeServiceWorkerFetchOutcome::Suspended) => {
                        Err(NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "Service Worker fetch is awaiting a browser WindowClient"
                                .into(),
                        })
                    }
                    Err(error) => Err(error),
                };
                match fetch {
                    Ok(fetch) => json!({
                        "kind": "fetched",
                        "id": id,
                        "url": fetch.url,
                        "status": fetch.status,
                        "status_text": fetch.status_text,
                        "content_type": fetch.content_type,
                        "headers": fetch.headers,
                        "redirected": fetch.redirected,
                        "opaque": fetch.opaque,
                        "opaque_redirect": fetch.opaque_redirect,
                        "body_base64": base64::engine::general_purpose::STANDARD
                            .encode(fetch.body),
                    }),
                    Err(error) => content_error_response(id, error),
                }
            }
            "navigation_policy" if protocol_matches(&request) && running => {
                let document_url = request
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process navigation policy owner URL",
                            "must be text",
                        )
                    })?;
                let target_url = request
                    .get("target_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process navigation policy target URL",
                            "must be text",
                        )
                    })?;
                if request.get("policy").and_then(Value::as_str) != Some("navigate-to") {
                    content_error_response(
                        id,
                        NativeEngineError::invalid(
                            "content-process navigation policy",
                            "must use the navigate-to policy kind",
                        ),
                    )
                } else {
                    match resource_loader.as_mut() {
                        None => content_error_response(
                            id,
                            NativeEngineError::Worker {
                                operation: "content process navigation policy".into(),
                                reason: "content process has no committed document".into(),
                            },
                        ),
                        Some(loader) => {
                            let report = request
                                .get("report")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            let allowed = if report {
                                loader.allows_navigation(
                                    document_url,
                                    target_url,
                                    NativeNavigationPolicyKind::NavigateTo,
                                )?
                            } else {
                                loader.allows_navigation_silent(
                                    document_url,
                                    target_url,
                                    NativeNavigationPolicyKind::NavigateTo,
                                )?
                            };
                            let csp_violations = if report {
                                loader.take_csp_violations()
                            } else {
                                Vec::new()
                            };
                            json!({
                                "kind": "navigation_policy",
                                "id": id,
                                "policy": "navigate-to",
                                "allowed": allowed,
                                "csp_violations": csp_violations,
                            })
                        }
                    }
                }
            }
            "script" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let source = request
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process script source", "must be text")
                    })?;
                let mut page_events: NativePageEventBatch = request
                    .get("page_events")
                    .map(|value| {
                        serde_json::from_value(value.clone()).map_err(|_| {
                            NativeEngineError::invalid(
                                "content-process page events",
                                "must be a valid native event batch",
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or_default();
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata_and_dialog_handler(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                        Arc::clone(&dialog_handler),
                    ) {
                        Ok(runtime) => {
                            runtime.set_environment(environment.clone());
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            runtime.set_frame_script_bindings(frame_script_bindings.clone());
                            runtime.set_scroll_offset(scroll_offset);
                            runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_scroll_offset(scroll_offset);
                runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                let Some(committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let owner_value = request.get("owner").ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process script owner",
                        NativeWorkerFailureKind::Protocol,
                        "parent omitted the captured script owner",
                    )
                })?;
                let owner =
                    decode_content_cookie_owner(owner_value, "content process script owner")?;
                if owner.context_id != storage_context_id
                    || owner.frame_id != frame_id
                    || owner.generation != current.generation()
                    || without_fragment(&owner.document_url) != without_fragment(&committed_url)
                {
                    let response = content_error_response(
                        id,
                        NativeEngineError::worker_failure(
                            "content process script owner",
                            NativeWorkerFailureKind::Protocol,
                            "script owner does not match the active document",
                        ),
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let document_cookie = request
                    .get("document_cookie")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process script cookie projection",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted the visible cookie projection",
                        )
                    })?;
                if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
                    return Err(NativeEngineError::limit(
                        "parent document.cookie projection",
                        MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                        document_cookie.len(),
                    ));
                }
                runtime.set_cookie_state(document_cookie.to_owned());
                parent_document_cookie_projection = Some(document_cookie.to_owned());
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process Worker scheduling".into(),
                        reason: "content process resource loader is unavailable".into(),
                    });
                };
                runtime.set_inline_script_policy(loader.inline_script_policy(&committed_url)?);
                {
                    let mut parent_fetch_broker = NativeContentFetchBroker {
                        captured_load_fetches: false,
                        request_id: id.as_u64().ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process script request ID",
                                "must be an unsigned integer",
                            )
                        })?,
                        owner: owner.clone(),
                        runtime: Some(runtime),
                        stdout: &mut stdout,
                        ipc_requests: &mut ipc_request_rx,
                        cancelled_parent_fetches: &mut cancelled_parent_fetches,
                        document_cookie_projection: &mut parent_document_cookie_projection,
                        next_content_resource_fetch_id: 0,
                        page_meta_content_security_policies: loader
                            .document_meta_content_security_policies(&committed_url)?,
                    };
                    service_workers
                        .run_next_parent_brokered_lifetime_fetch(loader, &mut parent_fetch_broker)
                        .await?;
                    service_workers
                        .run_due_timers_with_parent_fetch_broker(loader, &mut parent_fetch_broker)
                        .await?;
                    workers
                        .run_due_timers_with_parent_fetch_broker(loader, &mut parent_fetch_broker)
                        .await?;
                    process_worker_event_source_commands(
                        workers.take_event_source_commands(),
                        &mut worker_event_source_connections,
                        Some(&mut parent_fetch_broker),
                    )
                    .await?;
                    let _ = pump_parent_worker_event_source_stream(
                        &mut worker_event_source_connections,
                        Some(&mut parent_fetch_broker),
                        &mut worker_event_source_read_cursor,
                    )
                    .await?;
                    pump_worker_event_source_event(
                        &mut workers,
                        &mut worker_event_source_connections,
                        loader,
                        Some(&mut parent_fetch_broker),
                    )
                    .await?;
                    let mut worker_websocket_commands = pending_worker_websocket_commands
                        .drain(..)
                        .collect::<Vec<_>>();
                    worker_websocket_commands.extend(workers.take_websocket_commands());
                    process_worker_websocket_commands(
                        worker_websocket_commands,
                        &mut worker_websocket_connections,
                        Some(&mut parent_fetch_broker),
                    )
                    .await?;
                    pump_worker_websocket_event(
                        &mut workers,
                        &mut worker_websocket_connections,
                        loader,
                        Some(&mut parent_fetch_broker),
                        &mut worker_websocket_read_cursor,
                    )
                    .await?;
                    workers
                        .pump_fetch_stream_events_with_parent_fetch_broker(
                            loader,
                            &mut parent_fetch_broker,
                        )
                        .await?;
                }
                page_events
                    .worker_messages
                    .extend(pending_worker_messages.drain(..));
                page_events
                    .message_port_messages
                    .extend(pending_message_port_messages.drain(..));
                page_events
                    .service_worker_client_messages
                    .extend(pending_service_worker_client_messages.drain(..));
                page_events.worker_messages.extend(workers.take_messages());
                page_events
                    .message_port_messages
                    .extend(workers.take_message_port_messages());
                page_events
                    .message_port_messages
                    .extend(service_workers.take_message_port_messages());
                page_events
                    .service_worker_client_messages
                    .extend(service_workers.take_client_messages());
                let evaluation = runtime.evaluate_with_page_events(
                    source,
                    current,
                    &committed_url,
                    document_origin,
                    viewport,
                    &page_events,
                );
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process Worker scheduling".into(),
                        reason: "content process resource loader is unavailable".into(),
                    });
                };
                let mut parent_fetch_broker = NativeContentFetchBroker {
                    captured_load_fetches: false,
                    request_id: id.as_u64().ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process script request ID",
                            "must be an unsigned integer",
                        )
                    })?,
                    owner: owner.clone(),
                    runtime: Some(runtime),
                    stdout: &mut stdout,
                    ipc_requests: &mut ipc_request_rx,
                    cancelled_parent_fetches: &mut cancelled_parent_fetches,
                    document_cookie_projection: &mut parent_document_cookie_projection,
                    next_content_resource_fetch_id: 0,
                    page_meta_content_security_policies: loader
                        .document_meta_content_security_policies(&committed_url)?,
                };
                let service_worker_commands = runtime.take_service_worker_commands();
                let resolved_service_worker_commands =
                    if evaluation.is_ok() && !service_worker_commands.is_empty() {
                        let mut service_document = current.clone();
                        resolve_service_worker_commands(
                            service_worker_commands,
                            &mut service_workers,
                            loader,
                            runtime,
                            &mut service_document,
                            &committed_url,
                            document_origin,
                            viewport,
                            Some(&mut parent_fetch_broker),
                        )
                        .await?
                    } else {
                        Vec::new()
                    };
                pending_message_port_messages.extend(service_workers.take_message_port_messages());
                pending_service_worker_client_messages
                    .extend(service_workers.take_client_messages());
                let worker_commands = defer_shared_worker_commands(
                    runtime.take_worker_commands(),
                    external_shared_worker_routing,
                    document_origin,
                    &storage_context_id,
                    &frame_id,
                    current.generation(),
                    &mut pending_shared_worker_commands,
                )?;
                workers
                    .apply_commands_with_parent_fetch_broker(
                        worker_commands,
                        loader,
                        &committed_url,
                        &mut parent_fetch_broker,
                    )
                    .await?;
                let message_port_commands = runtime.take_message_port_commands();
                service_workers
                    .apply_page_message_port_commands(
                        message_port_commands.clone(),
                        loader,
                        Some(&mut parent_fetch_broker),
                    )
                    .await?;
                apply_worker_page_message_port_commands(
                    &mut workers,
                    message_port_commands,
                    loader,
                    external_shared_worker_routing,
                    Some(&mut parent_fetch_broker),
                )
                .await?;
                let mut worker_websocket_commands = pending_worker_websocket_commands
                    .drain(..)
                    .collect::<Vec<_>>();
                worker_websocket_commands.extend(workers.take_websocket_commands());
                process_worker_websocket_commands(
                    worker_websocket_commands,
                    &mut worker_websocket_connections,
                    Some(&mut parent_fetch_broker),
                )
                .await?;
                pump_worker_websocket_event(
                    &mut workers,
                    &mut worker_websocket_connections,
                    loader,
                    Some(&mut parent_fetch_broker),
                    &mut worker_websocket_read_cursor,
                )
                .await?;
                process_worker_event_source_commands(
                    workers.take_event_source_commands(),
                    &mut worker_event_source_connections,
                    Some(&mut parent_fetch_broker),
                )
                .await?;
                let _ = pump_parent_worker_event_source_stream(
                    &mut worker_event_source_connections,
                    Some(&mut parent_fetch_broker),
                    &mut worker_event_source_read_cursor,
                )
                .await?;
                pump_worker_event_source_event(
                    &mut workers,
                    &mut worker_event_source_connections,
                    loader,
                    Some(&mut parent_fetch_broker),
                )
                .await?;
                workers
                    .pump_fetch_stream_events_with_parent_fetch_broker(
                        loader,
                        &mut parent_fetch_broker,
                    )
                    .await?;
                pending_worker_messages.extend(workers.take_messages());
                pending_message_port_messages.extend(workers.take_message_port_messages());
                pending_page_message_port_commands
                    .extend(workers.take_page_message_port_commands());
                match evaluation.map(|mut evaluation| {
                    evaluation.commands.extend(resolved_service_worker_commands);
                    evaluation
                }) {
                    Ok(NativeScriptEvaluation {
                        value,
                        commands,
                        top_level_await_pending,
                        ..
                    }) if commands.is_empty()
                        && !top_level_await_pending
                        && websocket_connections.is_empty()
                        && event_source_connections.is_empty() =>
                    {
                        let frame_scripts = runtime.take_frame_script_events();
                        json!({
                            "kind":"evaluated",
                            "id":id,
                            "value":value,
                            "history": [],
                            "frame_scripts":frame_scripts,
                        })
                    }
                    Ok(NativeScriptEvaluation {
                        value,
                        commands,
                        top_level_await_pending,
                        ..
                    }) => {
                        let history = extract_history_commands(&commands);
                        if let Err(error) = apply_content_runtime_history(
                            &history,
                            &mut document_url,
                            document_origin,
                            runtime,
                        ) {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                        let script_url = document_url.clone().unwrap_or(committed_url.clone());
                        let has_websocket = commands.iter().any(|command| {
                            matches!(
                                command,
                                NativeScriptCommand::WebSocketOpen { .. }
                                    | NativeScriptCommand::WebSocketSend { .. }
                                    | NativeScriptCommand::WebSocketClose { .. }
                            )
                        });
                        let has_event_source = commands.iter().any(|command| {
                            matches!(
                                command,
                                NativeScriptCommand::EventSourceOpen { .. }
                                    | NativeScriptCommand::EventSourceClose { .. }
                            )
                        });
                        let has_open_background_transport = !websocket_connections.is_empty()
                            || !event_source_connections.is_empty();
                        let page_meta_content_security_policies = resource_loader
                            .as_ref()
                            .map(|loader| {
                                loader.document_meta_content_security_policies(&committed_url)
                            })
                            .transpose()?
                            .unwrap_or_default();
                        let result = resolve_script_fetches(
                            current,
                            runtime,
                            resource_loader.as_mut(),
                            Some(NativeContentFetchBroker {
                                captured_load_fetches: false,
                                request_id: id.as_u64().ok_or_else(|| {
                                    NativeEngineError::invalid(
                                        "content-process script request ID",
                                        "must be an unsigned integer",
                                    )
                                })?,
                                owner: owner.clone(),
                                runtime: Some(runtime),
                                stdout: &mut stdout,
                                ipc_requests: &mut ipc_request_rx,
                                cancelled_parent_fetches: &mut cancelled_parent_fetches,
                                document_cookie_projection: &mut parent_document_cookie_projection,
                                next_content_resource_fetch_id: 0,
                                page_meta_content_security_policies:
                                    page_meta_content_security_policies.clone(),
                            }),
                            &mut service_workers,
                            &mut websocket_connections,
                            &mut fetch_stream_connections,
                            &mut fetch_upload_connections,
                            &mut event_source_connections,
                            &script_url,
                            document_origin,
                            viewport,
                            top_level_await_pending
                                || has_websocket
                                || has_event_source
                                || has_open_background_transport,
                            NativeScriptEvaluation {
                                value: value.clone(),
                                commands,
                                top_level_await_pending,
                                worker_script_error: None,
                            },
                        )
                        .await;
                        pending_message_port_messages
                            .extend(service_workers.take_message_port_messages());
                        pending_service_worker_client_messages
                            .extend(service_workers.take_client_messages());
                        match result {
                            Ok((next, mutation, resolved_value)) => {
                                let value = resolved_value.unwrap_or(value);
                                let Some(loader) = resource_loader.as_mut() else {
                                    let response = content_error_response(
                                        id,
                                        NativeEngineError::Worker {
                                            operation: "dynamic Worker scheduling".into(),
                                            reason:
                                                "content process resource loader is unavailable"
                                                    .into(),
                                        },
                                    );
                                    write_value_frame(&mut stdout, &response).await?;
                                    continue;
                                };
                                let dynamic_worker_commands = defer_shared_worker_commands(
                                    runtime.take_worker_commands(),
                                    external_shared_worker_routing,
                                    document_origin,
                                    &storage_context_id,
                                    &frame_id,
                                    current.generation(),
                                    &mut pending_shared_worker_commands,
                                )?;
                                let mut parent_fetch_broker = NativeContentFetchBroker {
                                    captured_load_fetches: false,
                                    request_id: id.as_u64().ok_or_else(|| {
                                        NativeEngineError::invalid(
                                            "content-process script request ID",
                                            "must be an unsigned integer",
                                        )
                                    })?,
                                    owner: owner.clone(),
                                    runtime: Some(runtime),
                                    stdout: &mut stdout,
                                    ipc_requests: &mut ipc_request_rx,
                                    cancelled_parent_fetches: &mut cancelled_parent_fetches,
                                    document_cookie_projection:
                                        &mut parent_document_cookie_projection,
                                    next_content_resource_fetch_id: 0,
                                    page_meta_content_security_policies: loader
                                        .document_meta_content_security_policies(&committed_url)?,
                                };
                                workers
                                    .apply_commands_with_parent_fetch_broker(
                                        dynamic_worker_commands,
                                        loader,
                                        &document_url.clone().unwrap_or(committed_url.clone()),
                                        &mut parent_fetch_broker,
                                    )
                                    .await?;
                                let dynamic_message_port_commands =
                                    runtime.take_message_port_commands();
                                service_workers
                                    .apply_page_message_port_commands(
                                        dynamic_message_port_commands.clone(),
                                        loader,
                                        Some(&mut parent_fetch_broker),
                                    )
                                    .await?;
                                apply_worker_page_message_port_commands(
                                    &mut workers,
                                    dynamic_message_port_commands,
                                    loader,
                                    external_shared_worker_routing,
                                    Some(&mut parent_fetch_broker),
                                )
                                .await?;
                                let mut worker_websocket_commands =
                                    pending_worker_websocket_commands
                                        .drain(..)
                                        .collect::<Vec<_>>();
                                worker_websocket_commands.extend(workers.take_websocket_commands());
                                process_worker_websocket_commands(
                                    worker_websocket_commands,
                                    &mut worker_websocket_connections,
                                    Some(&mut parent_fetch_broker),
                                )
                                .await?;
                                pump_worker_websocket_event(
                                    &mut workers,
                                    &mut worker_websocket_connections,
                                    loader,
                                    Some(&mut parent_fetch_broker),
                                    &mut worker_websocket_read_cursor,
                                )
                                .await?;
                                process_worker_event_source_commands(
                                    workers.take_event_source_commands(),
                                    &mut worker_event_source_connections,
                                    Some(&mut parent_fetch_broker),
                                )
                                .await?;
                                let _ = pump_parent_worker_event_source_stream(
                                    &mut worker_event_source_connections,
                                    Some(&mut parent_fetch_broker),
                                    &mut worker_event_source_read_cursor,
                                )
                                .await?;
                                pump_worker_event_source_event(
                                    &mut workers,
                                    &mut worker_event_source_connections,
                                    loader,
                                    Some(&mut parent_fetch_broker),
                                )
                                .await?;
                                pending_worker_messages.extend(workers.take_messages());
                                pending_message_port_messages
                                    .extend(workers.take_message_port_messages());
                                pending_page_message_port_commands
                                    .extend(workers.take_page_message_port_commands());
                                if !mutation.history.is_empty() {
                                    let Some(base_url) = document_url.as_deref() else {
                                        let response = content_error_response(
                                            id,
                                            NativeEngineError::Worker {
                                                operation: "content process script history".into(),
                                                reason: "history mutation lost its document URL"
                                                    .into(),
                                            },
                                        );
                                        write_value_frame(&mut stdout, &response).await?;
                                        continue;
                                    };
                                    match resolve_content_history_document_url(
                                        &mutation.history,
                                        base_url,
                                        document_origin,
                                    ) {
                                        Ok(url) => document_url = Some(url),
                                        Err(error) => {
                                            let response = content_error_response(id, error);
                                            write_value_frame(&mut stdout, &response).await?;
                                            continue;
                                        }
                                    }
                                }
                                document = Some(next);
                                let frame_scripts = runtime.take_frame_script_events();
                                json!({
                                    "kind": "evaluated",
                                    "id": id,
                                    "value": value,
                                    "history": history,
                                    "mutation_history": mutation.history,
                                    "scroll_commands": mutation.scroll_commands,
                                    "frame_scripts": frame_scripts,
                                    "document_base64": base64::engine::general_purpose::STANDARD
                                        .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                                    "events": mutation.events.iter().map(|event| json!({
                                        "node_index": event.node_index,
                                        "kind": event_kind_text(event.kind),
                                    })).collect::<Vec<_>>(),
                                    "dialogs": mutation.dialogs,
                                    "navigation": mutation.navigation.as_ref().map(content_navigation_json),
                                })
                            }
                            Err(error) => content_error_response(id, error),
                        }
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_click_preflight" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let node_index = request
                    .get("action")
                    .and_then(|action| action.get("node_index"))
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process click target",
                            "must be a uint32",
                        )
                    })?;
                let modifiers = request
                    .get("action")
                    .and_then(|action| action.get("modifiers"))
                    .and_then(Value::as_u64)
                    .and_then(|value| u8::try_from(value).ok())
                    .unwrap_or(0);
                if modifiers > 15 {
                    let response = content_error_response(
                        id,
                        NativeEngineError::invalid(
                            "content-process click modifiers",
                            "must be a bit mask from 0 through 15",
                        ),
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata_and_dialog_handler(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                        Arc::clone(&dialog_handler),
                    ) {
                        Ok(runtime) => {
                            runtime.set_environment(environment.clone());
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_click_with_event_preflight(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    node_index,
                    modifiers,
                    resource_loader.as_mut(),
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "allowed": mutation.allowed,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(content_navigation_json),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_type_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process type action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process type target",
                            "must be a uint32",
                        )
                    })?;
                let text = action.get("text").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process type text", "must be text")
                })?;
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata_and_dialog_handler(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                        Arc::clone(&dialog_handler),
                    ) {
                        Ok(runtime) => {
                            runtime.set_environment(environment.clone());
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_type_with_event_bridge(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    node_index,
                    text,
                    resource_loader.as_mut(),
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_form_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process form action".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process form action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process form target",
                            "must be a uint32",
                        )
                    })?;
                let action_kind = action.get("kind").and_then(Value::as_str);
                let destination_node_index = action
                    .get("destination_node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok());
                let form_action = match action_kind {
                    Some("clear") => NativeFormAction::Clear,
                    Some("select") => NativeFormAction::Select(
                        action
                            .get("value")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process select value",
                                    "must be text",
                                )
                            })?
                            .to_owned(),
                    ),
                    Some("hover") => NativeFormAction::Hover,
                    Some("drag") => NativeFormAction::Drag {
                        destination_node_index: destination_node_index.ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process drag destination",
                                "must be a uint32",
                            )
                        })?,
                    },
                    Some("upload") => {
                        let files = serde_json::from_value::<Vec<NativeFile>>(
                            action.get("files").cloned().ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process upload files",
                                    "must be present",
                                )
                            })?,
                        )
                        .map_err(|_| {
                            NativeEngineError::invalid(
                                "content-process upload files",
                                "must be a valid native file list",
                            )
                        })?;
                        NativeFile::validate_many(&files)?;
                        NativeFormAction::Upload(files)
                    }
                    _ => {
                        let response = content_error_response(
                            id,
                            NativeEngineError::invalid(
                                "content-process form action",
                                "kind must be clear, select, hover, drag, or upload",
                            ),
                        );
                        write_value_frame(&mut stdout, &response).await?;
                        continue;
                    }
                };
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process form action".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process form action".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata_and_dialog_handler(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                        Arc::clone(&dialog_handler),
                    ) {
                        Ok(runtime) => {
                            runtime.set_environment(environment.clone());
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_form_action_with_event_bridge(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    node_index,
                    form_action,
                    resource_loader.as_mut(),
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_key_events" | "mutate_key_event" | "mutate_key_shortcut"
                if protocol_matches(&request) && running =>
            {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process key action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process key target", "must be a uint32")
                    })?;
                let key = action.get("key").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process key", "must be text")
                })?;
                let request_kind = request.get("kind").and_then(Value::as_str).unwrap_or("");
                let action_kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
                let is_legacy_key_press = request_kind == "mutate_key_events";
                let key_validation = if is_legacy_key_press {
                    validate_native_edit_key(key)
                } else {
                    validate_native_key(key)
                };
                if let Err(error) = key_validation {
                    let response = content_error_response(id, error);
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata_and_dialog_handler(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                        Arc::clone(&dialog_handler),
                    ) {
                        Ok(runtime) => {
                            runtime.set_environment(environment.clone());
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                let modifiers = action.get("modifiers").and_then(Value::as_i64).unwrap_or(0);
                if !(0..=15).contains(&modifiers) {
                    let response = content_error_response(
                        id,
                        NativeEngineError::invalid(
                            "content-process key modifiers",
                            "must be an integer mask from 0 through 15",
                        ),
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let result = match (request_kind, action_kind) {
                    ("mutate_key_event", "keydown" | "keyup") => {
                        let kind = if action_kind == "keydown" {
                            NativeEventKind::KeyDown
                        } else {
                            NativeEventKind::KeyUp
                        };
                        if pending_space_activation
                            .is_some_and(|(target, _)| target.generation() != current.generation())
                        {
                            pending_space_activation = None;
                        }
                        let target_id = NativeNodeId::from_parts(current.generation(), node_index);
                        let is_space = key == " ";
                        let pending = pending_space_activation;
                        let was_pending_space = kind == NativeEventKind::KeyUp
                            && is_space
                            && pending.is_some_and(|(pending_id, checkable_kind)| {
                                pending_id == target_id
                                    && match checkable_kind {
                                        Some(expected) => {
                                            current.keyboard_checkable_kind(target_id)
                                                == Some(expected)
                                        }
                                        None => {
                                            current.has_native_keyboard_button_activation(target_id)
                                        }
                                    }
                            });
                        let eligible_on_keydown = kind == NativeEventKind::KeyDown
                            && is_space
                            && (current.has_native_keyboard_button_activation(target_id)
                                || current.keyboard_checkable_kind(target_id).is_some());
                        let checkable_kind = current.keyboard_checkable_kind(target_id);
                        if is_space
                            && matches!(kind, NativeEventKind::KeyDown | NativeEventKind::KeyUp)
                        {
                            pending_space_activation = None;
                        }
                        let result = mutate_key_event_with_event_bridge(
                            current,
                            runtime,
                            &mut committed_url,
                            document_origin,
                            viewport,
                            node_index,
                            key,
                            kind,
                            modifiers,
                            was_pending_space,
                            resource_loader.as_mut(),
                        );
                        if eligible_on_keydown
                            && result.as_ref().is_ok_and(|(next, mutation)| {
                                mutation.allowed
                                    && next.focused_node() == target_id
                                    && match checkable_kind {
                                        Some(expected) => {
                                            next.keyboard_checkable_kind(target_id)
                                                == Some(expected)
                                        }
                                        None => {
                                            next.has_native_keyboard_button_activation(target_id)
                                        }
                                    }
                            })
                        {
                            pending_space_activation = Some((target_id, checkable_kind));
                        }
                        result
                    }
                    ("mutate_key_shortcut", "shortcut") => mutate_key_shortcut_with_event_bridge(
                        current,
                        runtime,
                        &mut committed_url,
                        document_origin,
                        viewport,
                        node_index,
                        key,
                        modifiers,
                        action
                            .get("apply_default")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        resource_loader.as_mut(),
                    ),
                    ("mutate_key_events", "") => mutate_key_with_event_bridge(
                        current,
                        runtime,
                        &mut committed_url,
                        document_origin,
                        viewport,
                        node_index,
                        key,
                        resource_loader.as_mut(),
                    ),
                    _ => Err(NativeEngineError::invalid(
                        "content-process key action",
                        "request kind must match keydown, keyup, or shortcut action",
                    )),
                };
                match result {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "allowed": mutation.allowed,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(content_navigation_json),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_before_unload" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(runtime) = javascript_runtime.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no JavaScript runtime".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_before_unload(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    resource_loader.as_mut(),
                ) {
                    Ok((next, mutation)) => {
                        let allowed = mutation.allowed;
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "allowed": allowed,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(content_navigation_json),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_lifecycle_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let event_values = request
                    .get("action")
                    .and_then(|action| action.get("events"))
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process lifecycle events",
                            "must be an array",
                        )
                    })?;
                let mut events = Vec::with_capacity(event_values.len());
                for value in event_values {
                    let event = value.as_str().and_then(parse_event_kind).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process lifecycle event",
                            "must be a supported lifecycle event",
                        )
                    })?;
                    if !matches!(
                        event,
                        NativeEventKind::PageHide
                            | NativeEventKind::Unload
                            | NativeEventKind::PageShow
                            | NativeEventKind::PopState
                    ) {
                        return Err(NativeEngineError::invalid(
                            "content-process lifecycle event",
                            "must be pagehide, unload, pageshow, or popstate",
                        ));
                    }
                    events.push(event);
                }
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(runtime) = javascript_runtime.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no JavaScript runtime".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_lifecycle_events(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    &events,
                    resource_loader.as_mut(),
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(content_navigation_json),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_hash_change" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process hashchange".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let old_url = request
                    .get("action")
                    .and_then(|action| action.get("old_url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("hashchange old URL", "must be text")
                    })?;
                let new_url = request
                    .get("action")
                    .and_then(|action| action.get("new_url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("hashchange new URL", "must be text")
                    })?;
                let mut committed_url = new_url.to_owned();
                validate_url_text("hashchange old URL", old_url)?;
                validate_url_text("hashchange new URL", new_url)?;
                if without_fragment(old_url) != without_fragment(new_url) || old_url == new_url {
                    return Err(NativeEngineError::invalid(
                        "hashchange URLs",
                        "must differ only by a non-empty fragment",
                    ));
                }
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process hashchange".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(runtime) = javascript_runtime.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process hashchange".into(),
                            reason: "content process has no JavaScript runtime".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(new_url, document_origin)),
                );
                match mutate_hash_change(
                    current,
                    runtime,
                    old_url,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    resource_loader.as_mut(),
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(content_navigation_json),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "close" if protocol_matches(&request) => {
                let (
                    storage_events,
                    indexed_db_changes,
                    _dialogs,
                    _popups,
                    _post_messages,
                    _window_closes,
                    _window_navigations,
                    _window_name,
                ) = sync_content_runtime_state(
                    javascript_runtime.as_ref(),
                    &mut storage_state,
                    &mut indexed_db_state,
                    &mut resource_loader,
                    document_url.as_deref(),
                    document_origin.as_ref(),
                )?;
                let cookie_writes = take_content_cookie_writes(
                    javascript_runtime.as_ref(),
                    &storage_context_id,
                    &frame_id,
                    document.as_ref().map(NativeDocument::generation),
                    document_url.as_deref(),
                )?;
                let cookie_changes = persist_content_profile(
                    storage_profile_path.as_deref(),
                    &storage_state,
                    &indexed_db_state,
                    &storage_events,
                    &indexed_db_changes,
                    &mut service_workers,
                    &mut resource_loader,
                )?;
                write_value_frame(
                    &mut stdout,
                    &json!({
                        "kind":"closed",
                        "id":id,
                        "cookie_changes":cookie_changes,
                        "cookie_writes":cookie_writes,
                    }),
                )
                .await?;
                return Ok(());
            }
            _ => content_error_response(
                id,
                NativeEngineError::Worker {
                    operation: "content process command".into(),
                    reason: format!(
                        "content process rejected kind {:?} with protocol {:?} while running={running}",
                        request.get("kind").and_then(Value::as_str),
                        request.get("protocol").and_then(Value::as_u64),
                    ),
                },
            ),
        };
        let is_action_mutation = response.get("kind").and_then(Value::as_str) == Some("mutated")
            && kind.starts_with("mutate_");
        if is_action_mutation && let Some(current_document_url) = document_url.clone() {
            let settlement = async {
                let owner = active_cookie_owner.clone().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent-brokered mutation resources",
                        NativeWorkerFailureKind::Protocol,
                        "the active document has no captured cookie owner",
                    )
                })?;
                let current_document = document.as_mut().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent-brokered mutation resources",
                        NativeWorkerFailureKind::Protocol,
                        "the active document disappeared during mutation settlement",
                    )
                })?;
                if owner.context_id != storage_context_id
                    || owner.frame_id != frame_id
                    || owner.generation != current_document.generation()
                {
                    return Err(NativeEngineError::worker_failure(
                        "parent-brokered mutation resources",
                        NativeWorkerFailureKind::Protocol,
                        "captured cookie owner does not match the mutated document",
                    ));
                }
                let runtime = javascript_runtime.as_ref().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent-brokered mutation resources",
                        NativeWorkerFailureKind::Protocol,
                        "the active document has no JavaScript runtime",
                    )
                })?;
                let loader = resource_loader.as_mut().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent-brokered mutation resources",
                        NativeWorkerFailureKind::Protocol,
                        "the active document has no resource loader",
                    )
                })?;
                let document_origin = document_origin.as_ref().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent-brokered mutation resources",
                        NativeWorkerFailureKind::Protocol,
                        "the active document has no origin",
                    )
                })?;
                let parent_fetch_broker = NativeContentFetchBroker {
                    captured_load_fetches: false,
                    request_id,
                    owner,
                    runtime: Some(runtime),
                    stdout: &mut stdout,
                    ipc_requests: &mut ipc_request_rx,
                    cancelled_parent_fetches: &mut cancelled_parent_fetches,
                    document_cookie_projection: &mut parent_document_cookie_projection,
                    next_content_resource_fetch_id: 0,
                    page_meta_content_security_policies: loader
                        .document_meta_content_security_policies(&current_document_url)?,
                };
                let (next, mutation, _) = resolve_script_fetches(
                    current_document,
                    runtime,
                    Some(loader),
                    Some(parent_fetch_broker),
                    &mut service_workers,
                    &mut websocket_connections,
                    &mut fetch_stream_connections,
                    &mut fetch_upload_connections,
                    &mut event_source_connections,
                    &current_document_url,
                    document_origin,
                    viewport,
                    false,
                    NativeScriptEvaluation {
                        value: Value::Null,
                        commands: Vec::new(),
                        top_level_await_pending: false,
                        worker_script_error: None,
                    },
                )
                .await?;
                *current_document = next;
                Ok(mutation)
            }
            .await;
            match settlement {
                Ok(mutation) => {
                    let Some(current_document) = document.as_ref() else {
                        return Err(NativeEngineError::worker_failure(
                            "parent-brokered mutation resources",
                            NativeWorkerFailureKind::Protocol,
                            "the active document disappeared after resource settlement",
                        ));
                    };
                    let encoded_document = serde_json::to_vec(&current_document.to_content_wire())
                        .map_err(|_| NativeEngineError::Worker {
                            operation: "encode parent-brokered mutation resources".into(),
                            reason: "the mutated document could not be encoded".into(),
                        })?;
                    response["document_base64"] = Value::String(
                        base64::engine::general_purpose::STANDARD.encode(encoded_document),
                    );
                    append_content_mutation_response_effects(&mut response, &mutation)?;
                    if !mutation.history.is_empty() {
                        let document_origin = document_origin.as_ref().ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "parent-brokered mutation history",
                                NativeWorkerFailureKind::Protocol,
                                "the active document has no origin",
                            )
                        })?;
                        document_url = Some(resolve_content_history_document_url(
                            &mutation.history,
                            &current_document_url,
                            document_origin,
                        )?);
                    }
                }
                Err(error) => response = content_error_response(response_id, error),
            }
        }
        let is_document_mutation = response.get("document_base64").is_some()
            && matches!(
                response.get("kind").and_then(Value::as_str),
                Some("mutated") | Some("evaluated")
            );
        if is_document_mutation {
            let navigate_to_sources = match (resource_loader.as_ref(), document_url.as_deref()) {
                (Some(loader), Some(document_url)) => loader.navigation_sources_for_document(
                    document_url,
                    NativeNavigationPolicyKind::NavigateTo,
                )?,
                _ => None,
            };
            response["navigate_to_sources"] =
                serde_json::to_value(navigate_to_sources).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "content process navigation policy transfer".into(),
                        reason: "navigation policy source groups were not serializable".into(),
                    }
                })?;
            let csp_violations = resource_loader
                .as_mut()
                .map(NativeResourceLoader::take_csp_violations)
                .unwrap_or_default();
            response["csp_violations"] =
                serde_json::to_value(csp_violations).map_err(|_| NativeEngineError::Worker {
                    operation: "content process CSP violation transfer".into(),
                    reason: "CSP violation records were not serializable".into(),
                })?;
        }
        let (
            storage_events,
            indexed_db_changes,
            dialogs,
            popups,
            post_messages,
            window_closes,
            window_navigations,
            response_window_name,
        ) = sync_content_runtime_state(
            javascript_runtime.as_ref(),
            &mut storage_state,
            &mut indexed_db_state,
            &mut resource_loader,
            document_url.as_deref(),
            document_origin.as_ref(),
        )?;
        let cookie_writes = take_content_cookie_writes(
            javascript_runtime.as_ref(),
            &storage_context_id,
            &frame_id,
            document.as_ref().map(NativeDocument::generation),
            document_url.as_deref(),
        )?;
        if !cookie_writes.is_empty() {
            parent_document_cookie_projection = None;
        }
        response["cookie_writes"] =
            serde_json::to_value(cookie_writes).map_err(|_| NativeEngineError::Worker {
                operation: "encode content process cookie writes".into(),
                reason: "content process cookie writes could not be encoded".into(),
            })?;
        let mut service_worker_open_windows = pending_service_worker_open_windows
            .drain(..)
            .collect::<Vec<_>>();
        service_worker_open_windows.extend(decode_service_worker_open_window_requests(
            &response,
            "merge content process openWindow",
        )?);
        service_worker_open_windows.extend(service_workers.take_open_windows());
        if service_worker_open_windows.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process service worker openWindow requests",
                MAX_NATIVE_EFFECTS,
                service_worker_open_windows.len(),
            ));
        }
        let mut service_worker_client_messages = pending_external_service_worker_client_messages
            .drain(..)
            .collect::<Vec<_>>();
        service_worker_client_messages.extend(decode_service_worker_client_messages(
            &response,
            "merge content process client messages",
        )?);
        service_worker_client_messages.extend(service_workers.take_external_client_messages());
        if service_worker_client_messages.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "content-process service worker client messages",
                MAX_NATIVE_WORKER_MESSAGES,
                service_worker_client_messages.len(),
            ));
        }
        if javascript_runtime.is_some() {
            window_name = response_window_name.clone();
        }
        let cookie_changes = persist_content_profile(
            storage_profile_path.as_deref(),
            &storage_state,
            &indexed_db_state,
            &storage_events,
            &indexed_db_changes,
            &mut service_workers,
            &mut resource_loader,
        )?;
        if !cookie_changes.is_empty() {
            parent_document_cookie_projection = None;
        }
        merge_cookie_change_batch(
            &mut pending_lifetime_cookie_changes,
            cookie_changes,
            "content process cookie changes",
        )?;
        let cookie_changes = std::mem::take(&mut pending_lifetime_cookie_changes);
        let encoded_cookie_changes =
            serde_json::to_value(cookie_changes).map_err(|_| NativeEngineError::Worker {
                operation: "encode content process cookie changes".into(),
                reason: "content process cookie changes could not be encoded".into(),
            })?;
        let mut response_dialogs = decode_dialogs(&response, "merge content process dialogs")?;
        response_dialogs.extend(dialogs);
        if response_dialogs.len() > MAX_NATIVE_DIALOGS {
            return Err(NativeEngineError::limit(
                "content-process dialogs",
                MAX_NATIVE_DIALOGS,
                response_dialogs.len(),
            ));
        }
        let mut response_post_messages =
            decode_post_message_requests(&response, "merge content process postMessage")?;
        response_post_messages.extend(post_messages);
        if response_post_messages.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process postMessage requests",
                MAX_NATIVE_EFFECTS,
                response_post_messages.len(),
            ));
        }
        let mut response_page_message_port_commands = decode_page_message_port_commands(
            &response,
            "merge content process page MessagePort commands",
        )?;
        response_page_message_port_commands.extend(pending_page_message_port_commands.drain(..));
        if response_page_message_port_commands.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process page MessagePort commands",
                MAX_NATIVE_EFFECTS,
                response_page_message_port_commands.len(),
            ));
        }
        let response_shared_worker_commands =
            pending_shared_worker_commands.drain(..).collect::<Vec<_>>();
        if response_shared_worker_commands.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process SharedWorker commands",
                MAX_NATIVE_EFFECTS,
                response_shared_worker_commands.len(),
            ));
        }
        let mut response_window_closes =
            decode_window_close_requests(&response, "merge content process window close")?;
        response_window_closes.extend(window_closes);
        if response_window_closes.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process window close requests",
                MAX_NATIVE_EFFECTS,
                response_window_closes.len(),
            ));
        }
        let mut response_window_navigations = decode_window_navigation_requests(
            &response,
            "merge content process window navigation",
        )?;
        response_window_navigations.extend(window_navigations);
        if response_window_navigations.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process window navigation requests",
                MAX_NATIVE_EFFECTS,
                response_window_navigations.len(),
            ));
        }
        if let Some(object) = response.as_object_mut() {
            object.insert("cookie_changes".into(), encoded_cookie_changes);
            object.insert(
                "storage_events".into(),
                serde_json::to_value(storage_events).map_err(|_| NativeEngineError::Worker {
                    operation: "encode content process storage events".into(),
                    reason: "content process storage events could not be encoded".into(),
                })?,
            );
            object.insert(
                "indexed_db_changes".into(),
                serde_json::to_value(&indexed_db_changes).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process IndexedDB changes".into(),
                        reason: "content process IndexedDB changes could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "dialogs".into(),
                serde_json::to_value(response_dialogs).map_err(|_| NativeEngineError::Worker {
                    operation: "encode content process dialogs".into(),
                    reason: "content process dialogs could not be encoded".into(),
                })?,
            );
            object.insert(
                "popups".into(),
                serde_json::to_value(popups).map_err(|_| NativeEngineError::Worker {
                    operation: "encode content process popups".into(),
                    reason: "content process popups could not be encoded".into(),
                })?,
            );
            object.insert(
                "post_messages".into(),
                serde_json::to_value(response_post_messages).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process postMessage requests".into(),
                        reason: "content process postMessage requests could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "page_message_port_commands".into(),
                serde_json::to_value(response_page_message_port_commands).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process page MessagePort commands".into(),
                        reason: "content process page MessagePort commands could not be encoded"
                            .into(),
                    }
                })?,
            );
            object.insert(
                "shared_worker_commands".into(),
                serde_json::to_value(response_shared_worker_commands).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process SharedWorker commands".into(),
                        reason: "content process SharedWorker commands could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "window_closes".into(),
                serde_json::to_value(response_window_closes).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process window close requests".into(),
                        reason: "content process window close requests could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "window_navigations".into(),
                serde_json::to_value(response_window_navigations).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process window navigation requests".into(),
                        reason: "content process window navigation requests could not be encoded"
                            .into(),
                    }
                })?,
            );
            object.insert(
                "service_worker_client_messages".into(),
                serde_json::to_value(service_worker_client_messages).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process service worker client messages".into(),
                        reason:
                            "content process service worker client messages could not be encoded"
                                .into(),
                    }
                })?,
            );
            object.insert(
                "service_worker_open_windows".into(),
                serde_json::to_value(service_worker_open_windows).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process service worker openWindow requests"
                            .into(),
                        reason: "content process service worker openWindow requests could not be encoded"
                            .into(),
                    }
                })?,
            );
            object.insert("window_name".into(), Value::String(response_window_name));
        }
        let has_pending_effects = content_worker_effects_pending(
            &pending_worker_messages,
            &pending_worker_websocket_commands,
            &pending_message_port_messages,
            &pending_page_message_port_commands,
            &pending_shared_worker_commands,
            &pending_service_worker_client_messages,
            &pending_external_service_worker_client_messages,
            &pending_service_worker_open_windows,
            &pending_lifetime_cookie_changes,
        )? || service_workers.has_pending_parent_lifetime_fetches();
        write_value_frame(&mut stdout, &response).await?;
        write_async_effects_ready(
            &mut stdout,
            has_pending_effects,
            &mut async_effect_notification_pending,
            &mut next_async_effect_sequence,
            &storage_context_id,
            &frame_id,
        )
        .await?;
    }
}
