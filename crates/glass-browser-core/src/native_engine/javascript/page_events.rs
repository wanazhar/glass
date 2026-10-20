use super::*;

pub(super) fn dispatch_page_event_batch(
    ctx: rquickjs::Ctx<'_>,
    events: &NativePageEventBatch,
) -> Result<(), NativeEngineError> {
    validate_page_event_batch(events)?;

    if !events.csp_violations.is_empty() {
        let payload = serde_json::to_value(&events.csp_violations).map_err(|_| {
            NativeEngineError::Worker {
                operation: "serialize native CSP violations".into(),
                reason: "native CSP violation metadata could not be serialized".into(),
            }
        })?;
        let payload = native_structured_payload(&ctx, &payload, "native CSP violations")?;
        let dispatch: Function =
            ctx.globals()
                .get("__glassDispatchCspViolations")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native CSP violations".into(),
                    reason: format!(
                        "native CSP violation dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
        dispatch
            .call::<_, Value>((payload,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native CSP violations".into(),
                reason: format!(
                    "native CSP violation dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }

    if !events.host_events.is_empty() {
        let payload =
            serde_json::to_value(&events.host_events).map_err(|_| NativeEngineError::Worker {
                operation: "serialize native host events".into(),
                reason: "native host event metadata could not be serialized".into(),
            })?;
        let payload = native_structured_payload(&ctx, &payload, "native host events")?;
        let dispatch: Function =
            ctx.globals()
                .get("__glassDispatchHostEvents")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native host events".into(),
                    reason: format!(
                        "native host event dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
        dispatch
            .call::<_, Value>((payload,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native host events".into(),
                reason: format!(
                    "native host event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }

    // Keep the established task-source order: Service Worker client events,
    // MessagePort events, then dedicated/shared-worker events. Each function
    // call remains in the same QuickJS context and therefore shares the host
    // command buffer with the user script that follows it.
    for message in &events.service_worker_client_messages {
        let payload = native_message_payload(
            &ctx,
            &serde_json::json!({
                "worker_id": message.worker_id,
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native service-worker client event",
        )?;
        let dispatch: Function = ctx
            .globals()
            .get("__glassDispatchServiceWorkerClientMessage")
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native service-worker client event".into(),
                reason: format!(
                    "native service-worker client dispatcher was unavailable: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
        dispatch
            .call::<_, Value>((payload,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native service-worker client event".into(),
                reason: format!(
                    "native service-worker client event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    for message in &events.message_port_messages {
        if message.close {
            let dispatch: Function = ctx
                .globals()
                .get("__glassDispatchMessagePortCloseByBridge")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native page MessagePort close".into(),
                    reason: format!(
                        "native page MessagePort close dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            dispatch
                .call::<_, Value>((message.bridge_key.as_str(),))
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native page MessagePort close".into(),
                    reason: format!(
                        "native page MessagePort close dispatch failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            continue;
        }
        let payload = native_message_payload(
            &ctx,
            &serde_json::json!({
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native page MessagePort event",
        )?;
        let dispatch: Function = ctx
            .globals()
            .get("__glassDispatchMessagePortByBridge")
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native page MessagePort event".into(),
                reason: format!(
                    "native page MessagePort dispatcher was unavailable: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
        dispatch
            .call::<_, Value>((message.bridge_key.as_str(), payload))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native page MessagePort event".into(),
                reason: format!(
                    "native page MessagePort event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    for message in &events.worker_messages {
        let payload = native_message_payload(
            &ctx,
            &serde_json::json!({
                "data": &message.data,
                "error": &message.error,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native Worker event",
        )?;
        let dispatch: Function =
            ctx.globals()
                .get("__glassDispatchWorkerMessage")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native Worker event".into(),
                    reason: format!(
                        "native Worker dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
        dispatch
            .call::<_, Value>((message.worker_id, payload))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native Worker event".into(),
                reason: format!(
                    "native Worker event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    for message in &events.post_message_events {
        let payload = native_message_payload(
            &ctx,
            &serde_json::json!({
                "source_context_id": &message.source_context_id,
                "source_origin": &message.source_origin,
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native page message event",
        )?;
        let dispatch: Function = ctx
            .globals()
            .get("__glassDispatchMessage")
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native page message event".into(),
                reason: format!(
                    "native page message dispatcher was unavailable: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
        dispatch
            .call::<_, Value>((payload,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native page message event".into(),
                reason: format!(
                    "native page message event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    for batch in &events.frame_event_batches {
        let payload =
            serde_json::to_value(&batch.events).map_err(|_| NativeEngineError::Worker {
                operation: "serialize native frame event batch".into(),
                reason: "native frame event metadata could not be serialized".into(),
            })?;
        let payload = native_structured_payload(&ctx, &payload, "native frame event batch")?;
        let dispatch: Function =
            ctx.globals()
                .get("__glassDispatchFrameEvents")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native frame events".into(),
                    reason: format!(
                        "native frame event dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
        dispatch
            .call::<_, Value>((batch.frame_id.as_str(), payload))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native frame events".into(),
                reason: format!(
                    "native frame event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    for command in &events.frame_script_commands {
        let (global_name, value) = match command {
            NativeScriptCommand::FrameScriptBatch { commands } => {
                ("__glassQueueNativeCommands", serde_json::to_value(commands))
            }
            _ => ("__glassApplyNativeCommand", serde_json::to_value(command)),
        };
        let value = value.map_err(|_| NativeEngineError::Worker {
            operation: "serialize native frame script command".into(),
            reason: "native frame script command could not be serialized".into(),
        })?;
        let value = native_structured_payload(&ctx, &value, "native frame script command")?;
        let dispatch: Function =
            ctx.globals()
                .get(global_name)
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native frame script command".into(),
                    reason: format!(
                        "{global_name} was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
        dispatch
            .call::<_, Value>((value,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native frame script command".into(),
                reason: format!(
                    "native frame script command dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    for event in &events.hash_change_events {
        let payload = native_structured_payload(
            &ctx,
            &serde_json::json!([{
                "node_index": u32::MAX,
                "type": "hashchange",
                "bubbles": false,
                "cancelable": false,
                "old_url": &event.old_url,
                "new_url": &event.new_url,
            }]),
            "native hashchange event",
        )?;
        let dispatch: Function =
            ctx.globals()
                .get("__glassDispatchHostEvents")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native hashchange event".into(),
                    reason: format!(
                        "native hashchange dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
        dispatch
            .call::<_, Value>((payload,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "dispatch native hashchange event".into(),
                reason: format!(
                    "native hashchange event dispatch failed: {}",
                    CaughtError::from_error(&ctx, error)
                ),
            })?;
    }
    Ok(())
}

pub(super) fn dispatch_page_payload(
    ctx: rquickjs::Ctx<'_>,
    dispatch: NativePageDispatch<'_>,
) -> Result<(), NativeEngineError> {
    match dispatch {
        NativePageDispatch::Fetch {
            request_id,
            payload,
        } => dispatch_page_resolver(
            &ctx,
            "__glassResolveFetch",
            "native fetch response",
            request_id,
            payload,
        )?,
        NativePageDispatch::FontFaceInstall {
            request_id,
            payload,
        } => dispatch_page_resolver(
            &ctx,
            "__glassResolveFontFaceInstall",
            "native FontFace install acknowledgement",
            request_id,
            payload,
        )?,
        NativePageDispatch::WebSocket { socket_id, payload } => dispatch_page_resolver(
            &ctx,
            "__glassDispatchWebSocketEvent",
            "native WebSocket event",
            socket_id,
            payload,
        )?,
        NativePageDispatch::EventSource { source_id, payload } => dispatch_page_resolver(
            &ctx,
            "__glassDispatchEventSourceEvent",
            "native EventSource event",
            source_id,
            payload,
        )?,
        NativePageDispatch::FetchStream { stream_id, payload } => dispatch_page_resolver(
            &ctx,
            "__glassDispatchFetchStreamEvent",
            "native fetch response stream event",
            stream_id,
            payload,
        )?,
        NativePageDispatch::FetchUpload { stream_id, payload } => dispatch_page_resolver(
            &ctx,
            "__glassDispatchFetchUploadEvent",
            "native fetch request upload event",
            stream_id,
            payload,
        )?,
        NativePageDispatch::ServiceWorkerRegistration {
            request_id,
            payload,
        } => dispatch_page_resolver(
            &ctx,
            "__glassResolveServiceWorker",
            "native service worker response",
            request_id,
            payload,
        )?,
        NativePageDispatch::ScriptError { payload } => dispatch_page_callback(
            &ctx,
            "__glassDispatchScriptError",
            "native script error event",
            payload,
        )?,
    }
    Ok(())
}

pub(super) fn dispatch_page_resolver(
    ctx: &rquickjs::Ctx<'_>,
    global_name: &str,
    operation: &str,
    identifier: u32,
    payload: &serde_json::Value,
) -> Result<(), NativeEngineError> {
    let payload = native_structured_payload(ctx, payload, operation)?;
    let resolve: Function =
        ctx.globals()
            .get(global_name)
            .map_err(|error| NativeEngineError::Worker {
                operation: format!("dispatch {operation}"),
                reason: format!(
                    "{global_name} was unavailable: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
    resolve
        .call::<_, Value>((identifier, payload))
        .map_err(|error| NativeEngineError::Worker {
            operation: format!("dispatch {operation}"),
            reason: format!(
                "{operation} dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}

pub(super) fn dispatch_page_callback(
    ctx: &rquickjs::Ctx<'_>,
    global_name: &str,
    operation: &str,
    payload: &serde_json::Value,
) -> Result<(), NativeEngineError> {
    let payload = native_structured_payload(ctx, payload, operation)?;
    let dispatch: Function =
        ctx.globals()
            .get(global_name)
            .map_err(|error| NativeEngineError::Worker {
                operation: format!("dispatch {operation}"),
                reason: format!(
                    "{global_name} was unavailable: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
    dispatch
        .call::<_, Value>((payload,))
        .map_err(|error| NativeEngineError::Worker {
            operation: format!("dispatch {operation}"),
            reason: format!(
                "{operation} dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}

pub(super) fn dispatch_worker_event<'js>(
    ctx: rquickjs::Ctx<'js>,
    dispatch: NativeWorkerDispatch<'_>,
    service_worker_message_dispatch: Option<Function<'js>>,
) -> Result<bool, NativeEngineError> {
    Ok(match dispatch {
        NativeWorkerDispatch::SharedWorkerConnect { transfer_ports } => {
            let payload = native_message_payload(
                &ctx,
                &serde_json::json!({
                    "transfer_ports": transfer_ports,
                }),
                "native SharedWorker connection",
            )?;
            let dispatch: Function = ctx
                .globals()
                .get("__glassDispatchSharedWorkerConnect")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native SharedWorker connection".into(),
                    reason: format!(
                        "native SharedWorker connection dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            dispatch
                .call::<_, Value>((payload,))
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native SharedWorker connection".into(),
                    reason: format!(
                        "native SharedWorker connection dispatch failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            false
        }
        NativeWorkerDispatch::Message {
            data,
            transfer_ports,
            object_urls,
        } => {
            let payload = native_message_payload(
                &ctx,
                &serde_json::json!({
                    "data": data,
                    "transfer_ports": transfer_ports,
                    "object_urls": object_urls,
                }),
                "native service-worker message",
            )?;
            let dispatch: Function =
                ctx.globals()
                    .get("__glassDispatchWorkerMessage")
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "dispatch native service-worker message".into(),
                        reason: format!(
                            "native service-worker message dispatcher was unavailable: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
            dispatch
                .call::<_, Value>((payload,))
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native service-worker message".into(),
                    reason: format!(
                        "native service-worker message dispatch failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            false
        }
        NativeWorkerDispatch::MessagePort {
            bridge_key,
            data,
            transfer_ports,
            object_urls,
        } => {
            let payload = native_message_payload(
                &ctx,
                &serde_json::json!({
                    "data": data,
                    "transfer_ports": transfer_ports,
                    "object_urls": object_urls,
                }),
                "native service-worker MessagePort event",
            )?;
            let dispatch: Function = ctx
                .globals()
                .get("__glassDispatchMessagePortByBridge")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native service-worker MessagePort event".into(),
                    reason: format!(
                        "native service-worker MessagePort dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            dispatch
                .call::<_, Value>((bridge_key, payload))
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native service-worker MessagePort event".into(),
                    reason: format!(
                        "native service-worker MessagePort dispatch failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            false
        }
        NativeWorkerDispatch::MessagePortClose { bridge_key } => {
            let dispatch: Function = ctx
                .globals()
                .get("__glassDispatchMessagePortCloseByBridge")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "dispatch native MessagePort close".into(),
                    reason: format!(
                        "native Worker MessagePort close dispatcher was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            dispatch.call::<_, Value>((bridge_key,)).map_err(|error| {
                NativeEngineError::Worker {
                    operation: "dispatch native MessagePort close".into(),
                    reason: format!(
                        "native Worker MessagePort close dispatch failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                }
            })?;
            false
        }
        NativeWorkerDispatch::Fetch {
            request_id,
            payload,
        } => {
            dispatch_worker_resolver(
                &ctx,
                "__glassResolveWorkerFetch",
                "native Worker fetch response",
                request_id,
                payload,
                false,
            )?;
            false
        }
        NativeWorkerDispatch::FetchStream { stream_id, payload } => {
            dispatch_worker_resolver(
                &ctx,
                "__glassDispatchWorkerFetchStreamEvent",
                "native Worker fetch response stream event",
                stream_id,
                payload,
                false,
            )?;
            false
        }
        NativeWorkerDispatch::FetchUpload { stream_id, payload } => {
            dispatch_worker_resolver(
                &ctx,
                "__glassDispatchWorkerFetchUploadEvent",
                "native Worker fetch request upload event",
                stream_id,
                payload,
                false,
            )?;
            false
        }
        NativeWorkerDispatch::ServiceWorkerCache {
            request_id,
            payload,
        } => {
            dispatch_worker_resolver(
                &ctx,
                "__glassResolveServiceWorkerCache",
                "native service-worker cache response",
                request_id,
                payload,
                false,
            )?;
            false
        }
        NativeWorkerDispatch::ServiceWorkerOpenWindow {
            request_id,
            payload,
        } => {
            dispatch_worker_resolver(
                &ctx,
                "__glassResolveServiceWorkerOpenWindow",
                "native service-worker openWindow response",
                request_id,
                payload,
                false,
            )?;
            false
        }
        NativeWorkerDispatch::ServiceWorkerNavigationPreload {
            request_id,
            payload,
        } => {
            dispatch_worker_resolver(
                &ctx,
                "__glassResolveServiceWorkerNavigationPreload",
                "native ServiceWorker navigation preload response",
                request_id,
                payload,
                true,
            )?;
            false
        }
        NativeWorkerDispatch::ServiceWorkerFetch { payload } => {
            dispatch_worker_promise(
                &ctx,
                "__glassDispatchServiceWorkerFetch",
                "native service worker fetch",
                payload,
            )?;
            true
        }
        NativeWorkerDispatch::ServiceWorkerLifecycle { event_type } => {
            let payload = serde_json::Value::String(event_type.to_owned());
            dispatch_worker_promise(
                &ctx,
                "__glassDispatchServiceWorkerLifecycle",
                "native service worker lifecycle",
                &payload,
            )?;
            true
        }
        NativeWorkerDispatch::ServiceWorkerMessage { payload } => {
            let dispatcher =
                service_worker_message_dispatch.ok_or_else(|| NativeEngineError::Worker {
                    operation: "dispatch native service worker message".into(),
                    reason: "native Service Worker message dispatcher was unavailable".into(),
                })?;
            dispatch_worker_promise_with_function(
                &ctx,
                dispatcher,
                "native service worker message",
                payload,
            )?;
            true
        }
        NativeWorkerDispatch::WebSocket {
            socket_id,
            payload,
            csp_violations,
        } => {
            dispatch_worker_csp_violations(&ctx, csp_violations)?;
            dispatch_worker_resolver(
                &ctx,
                "__glassDispatchWorkerWebSocketEvent",
                "native Worker WebSocket event",
                socket_id,
                payload,
                false,
            )?;
            false
        }
        NativeWorkerDispatch::EventSource {
            source_id,
            payload,
            csp_violations,
        } => {
            dispatch_worker_csp_violations(&ctx, csp_violations)?;
            dispatch_worker_resolver(
                &ctx,
                "__glassDispatchWorkerEventSourceEvent",
                "native Worker EventSource event",
                source_id,
                payload,
                false,
            )?;
            false
        }
    })
}

pub(super) fn dispatch_worker_csp_violations(
    ctx: &rquickjs::Ctx<'_>,
    violations: &[NativeCspViolation],
) -> Result<(), NativeEngineError> {
    if violations.is_empty() {
        return Ok(());
    }
    let events = NativePageEventBatch {
        csp_violations: violations.to_vec(),
        ..NativePageEventBatch::default()
    };
    validate_page_event_batch(&events)?;
    let payload = serde_json::to_value(violations).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native Worker CSP violations".into(),
        reason: "native Worker CSP violation metadata could not be serialized".into(),
    })?;
    let payload = native_structured_payload(ctx, &payload, "native Worker CSP violations")?;
    let dispatch: Function = ctx
        .globals()
        .get("__glassDispatchWorkerCspViolations")
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native Worker CSP violations".into(),
            reason: format!(
                "native Worker CSP violation dispatcher was unavailable: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    dispatch
        .call::<_, Value>((payload,))
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native Worker CSP violations".into(),
            reason: format!(
                "native Worker CSP violation dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}

pub(super) fn dispatch_worker_promise(
    ctx: &rquickjs::Ctx<'_>,
    global_name: &str,
    operation: &str,
    payload: &serde_json::Value,
) -> Result<(), NativeEngineError> {
    let dispatch: Function =
        ctx.globals()
            .get(global_name)
            .map_err(|error| NativeEngineError::Worker {
                operation: format!("dispatch {operation}"),
                reason: format!(
                    "{global_name} was unavailable: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
    dispatch_worker_promise_with_function(ctx, dispatch, operation, payload)
}

pub(super) fn dispatch_worker_promise_with_function<'js>(
    ctx: &rquickjs::Ctx<'js>,
    dispatch: Function<'js>,
    operation: &str,
    payload: &serde_json::Value,
) -> Result<(), NativeEngineError> {
    let payload = native_structured_payload(ctx, payload, operation)?;
    let promise: Value =
        dispatch
            .call::<_, Value>((payload,))
            .map_err(|error| NativeEngineError::Worker {
                operation: format!("dispatch {operation}"),
                reason: format!(
                    "{operation} dispatch failed: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
    ctx.globals()
        .set("__glassPendingNativeWorkerPromise", promise)
        .map_err(|error| NativeEngineError::Worker {
            operation: format!("stage {operation}"),
            reason: format!(
                "{operation} promise could not be staged: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}

pub(super) fn dispatch_service_worker_clients(
    ctx: &rquickjs::Ctx<'_>,
    clients: &[serde_json::Value],
) -> Result<(), NativeEngineError> {
    let payload = serde_json::Value::Array(clients.to_vec());
    let encoded = serde_json::to_vec(&payload).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native service worker clients".into(),
        reason: "native service worker clients could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker clients",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES,
            encoded.len(),
        ));
    }
    let payload = native_structured_payload(ctx, &payload, "native service worker clients")?;
    let dispatch: Function = ctx
        .globals()
        .get("__glassSetServiceWorkerClients")
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native service worker clients".into(),
            reason: format!(
                "native service worker client dispatcher was unavailable: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    dispatch
        .call::<_, Value>((payload,))
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native service worker clients".into(),
            reason: format!(
                "native service worker client dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}

pub(super) fn dispatch_worker_resolver(
    ctx: &rquickjs::Ctx<'_>,
    global_name: &str,
    operation: &str,
    request_id: u32,
    payload: &serde_json::Value,
    require_resolution: bool,
) -> Result<(), NativeEngineError> {
    let payload = native_structured_payload(ctx, payload, operation)?;
    let resolve: Function =
        ctx.globals()
            .get(global_name)
            .map_err(|error| NativeEngineError::Worker {
                operation: format!("dispatch {operation}"),
                reason: format!(
                    "{global_name} was unavailable: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
    let resolved = resolve
        .call::<_, Value>((request_id, payload))
        .map_err(|error| NativeEngineError::Worker {
            operation: format!("dispatch {operation}"),
            reason: format!(
                "{operation} dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    if require_resolution && resolved.as_bool() != Some(true) {
        return Err(NativeEngineError::Worker {
            operation: format!("dispatch {operation}"),
            reason: "the matching pending request was not found".into(),
        });
    }
    Ok(())
}

pub fn validate_message_port_transfers(
    transfers: &[NativeMessagePortTransfer],
) -> Result<(), NativeEngineError> {
    if transfers.len() > MAX_NATIVE_WORKER_MESSAGES {
        return Err(NativeEngineError::limit(
            "native MessagePort transfers",
            MAX_NATIVE_WORKER_MESSAGES,
            transfers.len(),
        ));
    }
    let mut keys = BTreeSet::new();
    for transfer in transfers {
        validate_url_text("native MessagePort bridge key", &transfer.bridge_key)?;
        if transfer.bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                transfer.bridge_key.len(),
            ));
        }
        if transfer.port_id == 0 || transfer.peer_id == 0 {
            return Err(NativeEngineError::invalid(
                "native MessagePort transfer",
                "source and peer port ids must be positive",
            ));
        }
        if !keys.insert(&transfer.bridge_key) {
            return Err(NativeEngineError::invalid(
                "native MessagePort transfer",
                "a bridge key may appear only once in a transfer list",
            ));
        }
    }
    Ok(())
}

pub fn validate_page_message_port_command(
    command: &NativePageMessagePortCommand,
) -> Result<(), NativeEngineError> {
    validate_url_text("native MessagePort bridge key", &command.bridge_key)?;
    if command.bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native MessagePort bridge key",
            crate::limits::MAX_BACKEND_ID_BYTES,
            command.bridge_key.len(),
        ));
    }
    if command.close
        && (command.data != serde_json::Value::Null
            || !command.transfer_ports.is_empty()
            || !command.object_urls.is_empty())
    {
        return Err(NativeEngineError::invalid(
            "native page MessagePort close",
            "close commands must not carry data or transfers",
        ));
    }
    validate_message_port_transfers(&command.transfer_ports)?;
    validate_native_object_url_transfers(&command.object_urls)?;
    validate_native_message_payload(
        &serde_json::json!({
            "data": &command.data,
            "transfer_ports": &command.transfer_ports,
            "object_urls": &command.object_urls,
        }),
        "native page MessagePort command",
    )
}
