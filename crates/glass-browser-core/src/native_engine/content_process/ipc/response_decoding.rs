use super::*;

pub(in crate::native_engine::content_process) fn decode_fetch_response(
    response: &Value,
    id: u64,
) -> Result<NativeFetchResponse, NativeEngineError> {
    require_response_kind(response, "fetched", id, "parent-brokered content fetch")?;
    let url =
        response
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode parent Fetch response".into(),
                reason: "parent omitted the fetch URL".into(),
            })?;
    validate_url_text("parent Fetch response URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::Worker {
            operation: "decode parent Fetch response".into(),
            reason: "parent returned a non-HTTP(S) fetch URL".into(),
        });
    }
    let status = response
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode parent Fetch response".into(),
            reason: "parent returned an invalid HTTP status".into(),
        })?;
    let status_text = response
        .get("status_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let content_type = response
        .get("content_type")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "decode parent Fetch response".into(),
                    reason: "parent returned an invalid content type".into(),
                })
        })
        .transpose()?;
    let raw_headers = response
        .get("headers")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode parent Fetch response".into(),
            reason: "parent returned invalid response headers".into(),
        })?;
    if raw_headers.len() > MAX_NATIVE_RESPONSE_HEADERS {
        return Err(NativeEngineError::limit(
            "parent Fetch response headers",
            MAX_NATIVE_RESPONSE_HEADERS,
            raw_headers.len(),
        ));
    }
    let mut headers = Vec::with_capacity(raw_headers.len());
    let mut total_header_bytes = 0usize;
    for header in raw_headers {
        let pair = header
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode parent Fetch response".into(),
                reason: "parent returned an invalid response header pair".into(),
            })?;
        let name = pair[0]
            .as_str()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode parent Fetch response".into(),
                reason: "parent returned a non-text response header name".into(),
            })?
            .to_ascii_lowercase();
        let value = pair[1].as_str().ok_or_else(|| NativeEngineError::Worker {
            operation: "decode parent Fetch response".into(),
            reason: "parent returned a non-text response header value".into(),
        })?;
        if name == "set-cookie" || name == "set-cookie2" {
            return Err(NativeEngineError::worker_failure(
                "decode parent Fetch response",
                NativeWorkerFailureKind::Protocol,
                "parent exposed a forbidden cookie response header to the child",
            ));
        }
        if name.len() > MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES {
            return Err(NativeEngineError::limit(
                "parent Fetch response header name",
                MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES,
                name.len(),
            ));
        }
        reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            NativeEngineError::Worker {
                operation: "decode parent Fetch response".into(),
                reason: "parent returned an invalid response header name".into(),
            }
        })?;
        if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "parent Fetch response header value",
                MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                value.len(),
            ));
        }
        total_header_bytes = total_header_bytes
            .saturating_add(name.len())
            .saturating_add(value.len());
        if total_header_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
            return Err(NativeEngineError::limit(
                "parent Fetch response headers",
                MAX_NATIVE_RESPONSE_HEADER_BYTES,
                total_header_bytes,
            ));
        }
        headers.push((name, value.to_owned()));
    }
    let encoded_body = response
        .get("body_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode parent Fetch response".into(),
            reason: "parent omitted the fetch body".into(),
        })?;
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded_body)
        .map_err(|_| NativeEngineError::Worker {
            operation: "decode parent Fetch response".into(),
            reason: "parent returned an invalid fetch body".into(),
        })?;
    if body.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "parent-brokered fetch response",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            body.len(),
        ));
    }
    Ok(NativeFetchResponse {
        url: url.to_owned(),
        status,
        status_text,
        content_type,
        headers,
        body,
        redirected: response
            .get("redirected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        opaque: response
            .get("opaque")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        opaque_redirect: response
            .get("opaque_redirect")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(in crate::native_engine::content_process) fn decode_script_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentScriptResult, NativeEngineError> {
    require_response_kind(response, "evaluated", id, "content process script")?;
    let value = response
        .get("value")
        .cloned()
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process script".into(),
            reason: "content process omitted the script result".into(),
        })?;
    let encoded = serde_json::to_vec(&value).map_err(|_| NativeEngineError::Worker {
        operation: "decode content process script".into(),
        reason: "content process returned an unserializable script result".into(),
    })?;
    if encoded.len() > crate::native_engine::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES {
        return Err(NativeEngineError::limit(
            "content-process script result",
            crate::native_engine::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
            encoded.len(),
        ));
    }
    let storage_events = decode_storage_events(response, "decode content process script")?;
    let indexed_db_changes = decode_indexed_db_changes(response, "decode content process script")?;
    let dialogs = decode_dialogs(response, "decode content process script")?;
    let has_document = response.get("document_base64").is_some();
    let has_events = response.get("events").is_some();
    let mutation = if has_document || has_events {
        Some(decode_mutation_payload(
            response,
            "decode content process script",
        )?)
    } else {
        None
    };
    let has_mutation = mutation.is_some();
    let post_messages = decode_post_message_requests(response, "decode content process script")?;
    let page_message_port_commands = decode_page_message_port_commands(
        response,
        "decode content process page MessagePort commands",
    )?;
    let shared_worker_commands =
        decode_shared_worker_commands(response, "decode content process SharedWorker commands")?;
    let window_closes = decode_window_close_requests(response, "decode content process script")?;
    let window_navigations =
        decode_window_navigation_requests(response, "decode content process script")?;
    let service_worker_client_messages = decode_service_worker_client_messages(
        response,
        "decode content process service worker client messages",
    )?;
    let service_worker_open_windows =
        decode_service_worker_open_window_requests(response, "decode content process script")?;
    let service_worker_fetch_resumed = response
        .get("service_worker_fetch_resumed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let frame_scripts = decode_frame_script_requests(response, "decode content process script")?;
    let window_name = decode_window_name(response, "decode content process script")?;
    let history = decode_history_commands(response, "decode content process script")?;
    Ok(NativeContentScriptResult {
        value,
        storage_events: if has_mutation {
            Vec::new()
        } else {
            storage_events
        },
        mutation,
        history,
        frame_scripts,
        indexed_db_changes,
        dialogs: if has_mutation { Vec::new() } else { dialogs },
        popups: if has_mutation {
            Vec::new()
        } else {
            decode_popup_requests(response, "decode content process script")?
        },
        post_messages: if has_mutation {
            Vec::new()
        } else {
            post_messages
        },
        page_message_port_commands,
        shared_worker_commands,
        window_closes: if has_mutation {
            Vec::new()
        } else {
            window_closes
        },
        window_navigations: if has_mutation {
            Vec::new()
        } else {
            window_navigations
        },
        service_worker_client_messages,
        service_worker_open_windows,
        service_worker_fetch_resumed,
        window_name,
    })
}

pub(in crate::native_engine::content_process) fn decode_frame_script_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeFrameScriptRequest>, NativeEngineError> {
    let Some(value) = response.get("frame_scripts") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid frame script requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process frame script requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests =
        serde_json::from_value::<Vec<NativeFrameScriptRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed frame script requests".into(),
            }
        })?;
    for request in &requests {
        validate_context_id(&request.frame_id)?;
        validate_context_id(&request.source_frame_id)?;
        if matches!(
            request.command.as_ref(),
            NativeScriptCommand::FrameScript { .. }
        ) {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned a nested frame script request".into(),
            });
        }
        let encoded = serde_json::to_vec(request.command.as_ref()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an unserializable frame script command".into(),
            }
        })?;
        if encoded.len() > crate::native_engine::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process frame script command",
                crate::native_engine::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
                encoded.len(),
            ));
        }
    }
    Ok(requests)
}

pub(in crate::native_engine::content_process) fn decode_window_name(
    response: &Value,
    _operation: &str,
) -> Result<String, NativeEngineError> {
    let value = response
        .get("window_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    validate_window_name(value)?;
    Ok(value.to_owned())
}

pub(in crate::native_engine::content_process) fn decode_window_close_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeWindowCloseRequest>, NativeEngineError> {
    let Some(value) = response.get("window_closes") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid window close requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process window close requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests =
        serde_json::from_value::<Vec<NativeWindowCloseRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed window close requests".into(),
            }
        })?;
    for request in &requests {
        validate_url_text("content-process window close target", &request.target)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            validate_context_id(target_context_id)?;
        }
    }
    Ok(requests)
}

pub(in crate::native_engine::content_process) fn decode_window_navigation_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeWindowNavigationRequest>, NativeEngineError> {
    let Some(value) = response.get("window_navigations") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid window navigation requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process window navigation requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests = serde_json::from_value::<Vec<NativeWindowNavigationRequest>>(value.clone())
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed window navigation requests".into(),
        })?;
    for request in &requests {
        if request.target.is_empty() && request.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "content-process window navigation target",
                "must not be empty without a direct target context",
            ));
        }
        if !request.target.is_empty() {
            validate_url_text("content-process window navigation target", &request.target)?;
        }
        validate_url_text("content-process window navigation href", &request.href)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            validate_context_id(target_context_id)?;
        }
        validate_target_navigation_payload(
            request.method,
            request.body.as_ref(),
            request.body_content_type.as_deref(),
            "content-process window navigation payload",
        )?;
        validate_object_url_payload(
            request.object_url.as_ref(),
            "content-process window navigation object URL",
        )?;
    }
    Ok(requests)
}

pub(in crate::native_engine::content_process) fn decode_service_worker_open_window_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeServiceWorkerOpenWindowRequest>, NativeEngineError> {
    let Some(value) = response.get("service_worker_open_windows") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid service worker openWindow requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process service worker openWindow requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests =
        serde_json::from_value::<Vec<NativeServiceWorkerOpenWindowRequest>>(value.clone())
            .map_err(|_| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed service worker openWindow requests"
                    .into(),
            })?;
    for request in &requests {
        if request.request_id == 0 || request.worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "content-process service worker openWindow request",
                "request and worker ids must be positive",
            ));
        }
        validate_url_text(
            "content-process service worker openWindow URL",
            &request.url,
        )?;
    }
    Ok(requests)
}

pub(in crate::native_engine::content_process) fn decode_service_worker_client_messages(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let Some(value) = response.get("service_worker_client_messages") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid service worker client messages".into(),
    })?;
    if values.len() > MAX_NATIVE_WORKER_MESSAGES {
        return Err(NativeEngineError::limit(
            "content-process service worker client messages",
            MAX_NATIVE_WORKER_MESSAGES,
            values.len(),
        ));
    }
    let messages = serde_json::from_value::<Vec<NativeServiceWorkerClientMessage>>(value.clone())
        .map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned malformed service worker client messages".into(),
    })?;
    for message in &messages {
        if message.worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "content-process service worker client message worker id",
                "must be positive",
            ));
        }
        if message.client_id.is_empty() {
            return Err(NativeEngineError::invalid(
                "content-process service worker client message id",
                "must not be empty",
            ));
        }
        if message.client_id.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "content-process service worker client message id",
                crate::limits::MAX_BACKEND_ID_BYTES,
                message.client_id.len(),
            ));
        }
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_object_url_transfers(&message.object_urls)?;
        let encoded = serde_json::to_vec(&message.data).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "service worker client message data could not be serialized".into(),
        })?;
        if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "content-process service worker client message data",
                MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "content-process service worker client message",
        )?;
    }
    Ok(messages)
}

pub(in crate::native_engine::content_process) fn decode_window_proxy_updates(
    value: Option<&Value>,
    operation: &str,
) -> Result<Vec<NativeWindowProxyUpdate>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process received invalid WindowProxy updates".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process WindowProxy updates",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let updates =
        serde_json::from_value::<Vec<NativeWindowProxyUpdate>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process received malformed WindowProxy updates".into(),
            }
        })?;
    for update in &updates {
        if update.cache_key.len() > crate::native_engine::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES
        {
            return Err(NativeEngineError::limit(
                "content-process WindowProxy cache key",
                crate::native_engine::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
                update.cache_key.len(),
            ));
        }
        validate_context_id(&update.target_context_id)?;
        validate_url_text("content-process WindowProxy URL", &update.href)?;
        validate_window_name(&update.name)?;
    }
    Ok(updates)
}

pub(in crate::native_engine::content_process) fn decode_dialogs(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeDialog>, NativeEngineError> {
    let Some(value) = response.get("dialogs") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid dialogs".into(),
    })?;
    if values.len() > MAX_NATIVE_DIALOGS {
        return Err(NativeEngineError::limit(
            "content-process dialogs",
            MAX_NATIVE_DIALOGS,
            values.len(),
        ));
    }
    let dialogs = serde_json::from_value::<Vec<NativeDialog>>(value.clone()).map_err(|_| {
        NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed dialogs".into(),
        }
    })?;
    for dialog in &dialogs {
        if !matches!(dialog.dialog_type.as_str(), "alert" | "confirm" | "prompt") {
            return Err(NativeEngineError::invalid(
                "content-process dialog type",
                "must be alert, confirm, or prompt",
            ));
        }
        if dialog.message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
            || dialog
                .default_value
                .as_ref()
                .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "content-process dialog text",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                dialog
                    .default_value
                    .as_ref()
                    .map_or(dialog.message.len(), String::len),
            ));
        }
    }
    Ok(dialogs)
}

pub(in crate::native_engine::content_process) fn decode_storage_events(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeStorageEvent>, NativeEngineError> {
    let Some(value) = response.get("storage_events") else {
        return Ok(Vec::new());
    };
    decode_storage_event_value(value, operation)
}

pub(in crate::native_engine::content_process) fn decode_indexed_db_changes(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeIndexedDbChange>, NativeEngineError> {
    let Some(value) = response.get("indexed_db_changes") else {
        return Ok(Vec::new());
    };
    let changes: Vec<NativeIndexedDbChange> =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed IndexedDB changes".into(),
        })?;
    if changes.len() > MAX_NATIVE_INDEXED_DB_CHANGES {
        return Err(NativeEngineError::limit(
            "content-process IndexedDB changes",
            MAX_NATIVE_INDEXED_DB_CHANGES,
            changes.len(),
        ));
    }
    Ok(changes)
}

pub(in crate::native_engine::content_process) fn decode_storage_event_value(
    value: &Value,
    operation: &str,
) -> Result<Vec<NativeStorageEvent>, NativeEngineError> {
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid storage events".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process storage events",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let events =
        serde_json::from_value::<Vec<NativeStorageEvent>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed storage events".into(),
            }
        })?;
    for event in &events {
        if !matches!(event.scope.as_str(), "local" | "session") {
            return Err(NativeEngineError::invalid(
                "content-process storage event scope",
                "must be local or session",
            ));
        }
        if event.storage_key.is_empty() || event.storage_key.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process storage event key",
                MAX_NATIVE_SCRIPT_BYTES,
                event.storage_key.len(),
            ));
        }
        validate_context_id(&event.source_context_id)?;
        if event.url.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process storage event URL",
                MAX_NATIVE_SCRIPT_BYTES,
                event.url.len(),
            ));
        }
        for (field, value) in [
            ("key", event.key.as_deref()),
            ("old value", event.old_value.as_deref()),
            ("new value", event.new_value.as_deref()),
        ] {
            if value.is_some_and(|value| value.len() > MAX_NATIVE_SCRIPT_BYTES) {
                return Err(NativeEngineError::limit(
                    format!("content-process storage event {field}"),
                    MAX_NATIVE_SCRIPT_BYTES,
                    value.map_or(0, str::len),
                ));
            }
        }
        if event.key.is_none() && event.new_value.is_some() {
            return Err(NativeEngineError::invalid(
                "content-process storage event",
                "a clear event must not contain a new value",
            ));
        }
    }
    Ok(events)
}

pub(in crate::native_engine::content_process) fn decode_document_wire(
    document_bytes: &[u8],
    operation: &str,
) -> Result<NativeDocumentWire, NativeEngineError> {
    if document_bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            document_bytes.len(),
        ));
    }
    serde_json::from_slice(document_bytes).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned an invalid document snapshot".into(),
    })
}

pub(in crate::native_engine::content_process) async fn load_font_faces(
    document: &mut NativeDocument,
    runtime: Option<&NativeJavaScriptRuntime>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    let mut resources = Vec::new();
    let system_fonts = NativeFontBook::system();
    for rule in document
        .font_face_rules()
        .iter()
        .take(MAX_NATIVE_FONT_FACES)
    {
        for source in &rule.sources {
            let bytes = match source {
                NativeFontFaceSource::Local(family) => system_fonts.local_font_bytes(
                    family,
                    FontWeightValue::from_numeric(rule.weight.nominal())
                        .unwrap_or(FontWeightValue::Normal),
                    rule.style,
                ),
                NativeFontFaceSource::Url(source) => {
                    let object_url = runtime
                        .map(|runtime| runtime.object_url_resource(source))
                        .transpose()?
                        .flatten();
                    if object_url.is_none() && is_parent_owned_network_target(document_url, source)
                    {
                        if let Some(broker) = parent_fetch_broker.as_deref_mut() {
                            refresh_parent_broker_meta_csp(loader, Some(broker))?;
                            broker
                                .load_font_response(document_url, source)
                                .await
                                .ok()
                                .flatten()
                                .map(|response| response.body)
                        } else {
                            None
                        }
                    } else {
                        loader
                            .load_font_async(document_url, source, object_url.as_ref())
                            .await
                            .ok()
                            .flatten()
                    }
                }
            };
            if let Some(bytes) = bytes {
                resources.push(NativeFontFaceResource::from_rule(rule, bytes));
                break;
            }
        }
    }
    document.set_font_resources(resources)
}
