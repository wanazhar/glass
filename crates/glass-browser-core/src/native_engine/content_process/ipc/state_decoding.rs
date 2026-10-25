use super::*;

pub(in crate::native_engine::content_process) fn require_response_kind(
    response: &Value,
    expected: &str,
    expected_id: u64,
    operation: &str,
) -> Result<(), NativeEngineError> {
    if response.get("kind").and_then(Value::as_str) == Some(expected)
        && response.get("id").and_then(Value::as_u64) == Some(expected_id)
    {
        return Ok(());
    }
    let actual_kind = response
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    let actual_id = response.get("id").and_then(Value::as_u64);
    let child_reason = response.get("reason").and_then(Value::as_str);
    let detail = match child_reason {
        Some(reason) => format!("; child reason: {reason}"),
        None => String::new(),
    };
    Err(NativeEngineError::worker_failure(
        operation,
        NativeWorkerFailureKind::Protocol,
        format!(
            "expected {expected} response for request {expected_id}, got {actual_kind} for request {actual_id:?}{detail}"
        ),
    ))
}

pub(in crate::native_engine::content_process) fn decode_load_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentLoadResult, NativeEngineError> {
    if response.get("kind").and_then(Value::as_str) == Some("service_worker_fetch_suspended") {
        require_response_kind(
            response,
            "service_worker_fetch_suspended",
            id,
            "content process service worker fetch suspension",
        )?;
        return Ok(NativeContentLoadResult::Suspended(
            decode_service_worker_open_window_requests(
                response,
                "decode content process service worker fetch suspension",
            )?,
        ));
    }
    require_response_kind(response, "loaded", id, "content process load")?;
    let url =
        response
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process load".into(),
                reason: "content process omitted the final URL".into(),
            })?;
    validate_url_text("content process final URL", url)?;
    let generation = response
        .get("generation")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|generation| *generation != 0)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process omitted a valid document generation".into(),
        })?;
    let is_blob_url = without_fragment(url).starts_with("blob:");
    if !is_network_url(without_fragment(url)) && !is_blob_url {
        return Err(NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned an unsupported non-HTTP(S) final URL".into(),
        });
    }
    let encoded_document = response
        .get("document_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process omitted the document snapshot".into(),
        })?;
    let document_bytes =
        decode_document_snapshot_base64(encoded_document, "decode content process load")?;
    if document_bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            document_bytes.len(),
        ));
    }
    let document = decode_document_wire(&document_bytes, "decode content process load")?;
    let events = decode_event_payload(response, "decode content process load")?;
    let scroll_commands = decode_scroll_commands(response, "decode content process load")?;
    let origin = if is_blob_url {
        NativeOrigin::from_blob_url(without_fragment(url))?
    } else {
        let origin_url =
            url::Url::parse(without_fragment(url)).map_err(|_| NativeEngineError::Worker {
                operation: "decode content process load".into(),
                reason: "content process returned invalid final URL syntax".into(),
            })?;
        NativeOrigin::from_url(&origin_url)?
    };
    let navigation = response
        .get("navigation")
        .filter(|value| !value.is_null())
        .map(|value| decode_content_navigation(value, "decode content process load"))
        .transpose()?;
    let storage_events = decode_storage_events(response, "decode content process load")?;
    let indexed_db_changes = decode_indexed_db_changes(response, "decode content process load")?;
    let dialogs = decode_dialogs(response, "decode content process load")?;
    let popups = decode_popup_requests(response, "decode content process load")?;
    let post_messages = decode_post_message_requests(response, "decode content process load")?;
    let page_message_port_commands = decode_page_message_port_commands(
        response,
        "decode content process page MessagePort commands",
    )?;
    let shared_worker_commands =
        decode_shared_worker_commands(response, "decode content process SharedWorker commands")?;
    let window_closes = decode_window_close_requests(response, "decode content process load")?;
    let window_navigations =
        decode_window_navigation_requests(response, "decode content process load")?;
    let service_worker_client_messages = decode_service_worker_client_messages(
        response,
        "decode content process service worker client messages",
    )?;
    let service_worker_open_windows =
        decode_service_worker_open_window_requests(response, "decode content process load")?;
    let window_name = decode_window_name(response, "decode content process load")?;
    let frame_sources = decode_frame_sources(response, "decode content process load")?;
    let navigate_to_sources = decode_navigation_sources(response, "decode content process load")?;
    Ok(NativeContentLoadResult::Loaded(Box::new(
        NativeContentLoad {
            url: url.into(),
            generation,
            origin,
            document,
            frame_sources,
            navigate_to_sources,
            events,
            csp_violations: Vec::new(),
            scroll_commands,
            navigation,
            storage_events,
            indexed_db_changes,
            dialogs,
            popups,
            post_messages,
            page_message_port_commands,
            shared_worker_commands,
            window_closes,
            window_navigations,
            service_worker_client_messages,
            service_worker_open_windows,
            window_name,
        },
    )))
}

pub(in crate::native_engine::content_process) fn decode_navigation_policy_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentNavigationPolicy, NativeEngineError> {
    require_response_kind(
        response,
        "navigation_policy",
        id,
        "content process navigation policy",
    )?;
    if response.get("policy").and_then(Value::as_str) != Some("navigate-to") {
        return Err(NativeEngineError::worker_failure(
            "decode content process navigation policy",
            NativeWorkerFailureKind::Protocol,
            "content process returned an unexpected navigation policy kind",
        ));
    }
    let allowed = response
        .get("allowed")
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process navigation policy".into(),
            reason: "content process omitted the navigation policy decision".into(),
        })?;
    let value = response
        .get("csp_violations")
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process navigation policy".into(),
            reason: "content process omitted navigation policy observations".into(),
        })?;
    let csp_violations: Vec<NativeCspViolation> =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: "decode content process navigation policy".into(),
            reason: "content process returned invalid navigation policy observations".into(),
        })?;
    if csp_violations.len() > MAX_NATIVE_CSP_VIOLATIONS {
        return Err(NativeEngineError::limit(
            "content-process navigation policy observations",
            MAX_NATIVE_CSP_VIOLATIONS,
            csp_violations.len(),
        ));
    }
    Ok(NativeContentNavigationPolicy {
        allowed,
        csp_violations,
    })
}

pub(in crate::native_engine::content_process) fn decode_frame_sources(
    response: &Value,
    operation: &str,
) -> Result<Option<Vec<Vec<String>>>, NativeEngineError> {
    decode_source_groups(
        response,
        "frame_sources",
        operation,
        "frame policy",
        MAX_CONTENT_FRAME_SOURCES,
    )
}

pub(in crate::native_engine::content_process) fn decode_navigation_sources(
    response: &Value,
    operation: &str,
) -> Result<Option<Vec<Vec<String>>>, NativeEngineError> {
    decode_source_groups(
        response,
        "navigate_to_sources",
        operation,
        "top-level navigation policy",
        MAX_CONTENT_NAVIGATION_SOURCES,
    )
}

pub(in crate::native_engine::content_process) fn decode_source_groups(
    response: &Value,
    field: &str,
    operation: &str,
    label: &str,
    max_groups: usize,
) -> Result<Option<Vec<Vec<String>>>, NativeEngineError> {
    let Some(value) = response.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let sources = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: format!("content process returned invalid {label} sources"),
    })?;
    if sources.len() > max_groups {
        return Err(NativeEngineError::limit(
            format!("content-process {label} sources"),
            max_groups,
            sources.len(),
        ));
    }
    let mut decoded = Vec::with_capacity(sources.len());
    let mut total_bytes = 0usize;
    let mut total_sources = 0usize;
    for group in sources {
        let group = group.as_array().ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: format!("content process returned an invalid {label} source group"),
        })?;
        if group.len() > MAX_CONTENT_POLICY_SOURCES_PER_GROUP {
            return Err(NativeEngineError::limit(
                format!("content-process {label} source group"),
                MAX_CONTENT_POLICY_SOURCES_PER_GROUP,
                group.len(),
            ));
        }
        total_sources = total_sources.saturating_add(group.len());
        if total_sources > MAX_CONTENT_POLICY_SOURCES_TOTAL {
            return Err(NativeEngineError::limit(
                format!("content-process {label} sources"),
                MAX_CONTENT_POLICY_SOURCES_TOTAL,
                total_sources,
            ));
        }
        let mut decoded_group = Vec::with_capacity(group.len());
        for source in group {
            let source = source.as_str().ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: format!("content process returned a non-text {label} source"),
            })?;
            if source.is_empty() || source.bytes().any(|byte| byte.is_ascii_control()) {
                return Err(NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: format!("content process returned an invalid {label} source"),
                });
            }
            if source.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
                return Err(NativeEngineError::limit(
                    format!("content-process {label} source"),
                    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                    source.len(),
                ));
            }
            let next_bytes = total_bytes.saturating_add(source.len());
            if next_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
                return Err(NativeEngineError::limit(
                    format!("content-process {label} sources"),
                    MAX_NATIVE_RESPONSE_HEADER_BYTES,
                    next_bytes,
                ));
            }
            total_bytes = next_bytes;
            decoded_group.push(source.to_owned());
        }
        decoded.push(decoded_group);
    }
    Ok(Some(decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_source_groups_accepts_larger_valid_groups_within_budgets() {
        let value = serde_json::json!({
            "frame_sources": [(0..96).map(|index| format!("https://frame-{index}.example")).collect::<Vec<_>>()],
        });

        let decoded = decode_frame_sources(&value, "test frame source decoding")
            .expect("valid frame policy group should decode")
            .expect("frame policy should be present");

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].len(), 96);
        assert_eq!(decoded[0][95], "https://frame-95.example");
    }

    #[test]
    fn decode_source_groups_rejects_excessive_source_count() {
        let value = serde_json::json!({
            "frame_sources": [(0..=MAX_CONTENT_POLICY_SOURCES_PER_GROUP)
                .map(|index| format!("https://frame-{index}.example"))
                .collect::<Vec<_>>()],
        });

        let error = decode_frame_sources(&value, "test frame source decoding")
            .expect_err("excessive frame policy group must fail closed");

        assert!(error.to_string().contains("source group"));
    }
}

pub(in crate::native_engine::content_process) fn decode_frame_script_bindings(
    value: Option<&Value>,
    operation: &str,
) -> Result<Vec<NativeFrameScriptBinding>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let encoded = serde_json::to_vec(value).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process frame script bindings were not serializable".into(),
    })?;
    if encoded.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process frame script bindings",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            encoded.len(),
        ));
    }
    let bindings: Vec<NativeFrameScriptBinding> =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned invalid frame script bindings".into(),
        })?;
    if bindings.len() > MAX_CONTENT_FRAME_SOURCES {
        return Err(NativeEngineError::limit(
            "content-process frame script bindings",
            MAX_CONTENT_FRAME_SOURCES,
            bindings.len(),
        ));
    }
    for binding in &bindings {
        if binding.frame_id.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an empty frame script ID".into(),
            });
        }
        validate_url_text("content-process frame script URL", &binding.url)?;
        if binding.origin.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an empty frame script origin".into(),
            });
        }
    }
    Ok(bindings)
}

pub(in crate::native_engine::content_process) fn decode_frame_script_context(
    value: Option<&Value>,
    operation: &str,
) -> Result<Option<NativeFrameScriptContext>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let encoded = serde_json::to_vec(value).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process frame script context was not serializable".into(),
    })?;
    if encoded.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process frame script context",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            encoded.len(),
        ));
    }
    let context: NativeFrameScriptContext =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned invalid frame script context".into(),
        })?;
    validate_context_id(&context.current_frame_id)?;
    if let Some(parent) = context.parent.as_ref() {
        validate_frame_script_window(parent, operation)?;
    }
    if let Some(top) = context.top.as_ref() {
        validate_frame_script_window(top, operation)?;
    }
    Ok(Some(context))
}

pub(in crate::native_engine::content_process) fn validate_frame_script_window(
    window: &NativeFrameScriptWindow,
    operation: &str,
) -> Result<(), NativeEngineError> {
    validate_context_id(&window.context_id)?;
    validate_url_text("content-process frame script window URL", &window.url)?;
    if window.origin.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an empty frame script window origin".into(),
        });
    }
    if window.children.len() > MAX_CONTENT_FRAME_SOURCES {
        return Err(NativeEngineError::limit(
            "content-process frame script window children",
            MAX_CONTENT_FRAME_SOURCES,
            window.children.len(),
        ));
    }
    for child in &window.children {
        validate_context_id(&child.frame_id)?;
        validate_url_text("content-process frame script child URL", &child.url)?;
        if child.origin.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an empty frame script child origin".into(),
            });
        }
        if child.children.len() > MAX_CONTENT_FRAME_SOURCES {
            return Err(NativeEngineError::limit(
                "content-process frame script descendants",
                MAX_CONTENT_FRAME_SOURCES,
                child.children.len(),
            ));
        }
    }
    Ok(())
}

pub(in crate::native_engine::content_process) fn decode_mutated_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentMutation, NativeEngineError> {
    require_response_kind(response, "mutated", id, "content process mutation")?;
    decode_mutation_payload(response, "decode content process mutation")
}

pub(in crate::native_engine::content_process) fn decode_mutation_payload(
    response: &Value,
    operation: &str,
) -> Result<NativeContentMutation, NativeEngineError> {
    let native_document_state_replayable = response
        .get("native_document_state_replayable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let document = if native_document_state_replayable {
        if !response.get("document_base64").is_some_and(Value::is_null) {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "replayable document response unexpectedly included a snapshot".into(),
            });
        }
        NativeDocumentWire {
            native_document_state_replayable: true,
            ..NativeDocumentWire::default()
        }
    } else {
        let encoded_document = response
            .get("document_base64")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process omitted the mutated document snapshot".into(),
            })?;
        let document_bytes = decode_document_snapshot_base64(encoded_document, operation)?;
        decode_document_wire(&document_bytes, operation)?
    };
    let navigate_to_sources = decode_navigation_sources(response, operation)?;
    let events = decode_event_payload(response, operation)?;
    let csp_violations = decode_csp_violations(response, operation)?;
    let navigation = response
        .get("navigation")
        .filter(|value| !value.is_null())
        .map(|value| decode_content_navigation(value, operation))
        .transpose()?;
    let storage_events = decode_storage_events(response, operation)?;
    let indexed_db_changes = decode_indexed_db_changes(response, operation)?;
    let dialogs = decode_dialogs(response, operation)?;
    let popups = decode_popup_requests(response, operation)?;
    let post_messages = decode_post_message_requests(response, operation)?;
    let window_closes = decode_window_close_requests(response, operation)?;
    let window_navigations = decode_window_navigation_requests(response, operation)?;
    let window_name = decode_window_name(response, operation)?;
    let history = decode_mutation_history(response, operation)?;
    let scroll_commands = decode_scroll_commands(response, operation)?;
    Ok(NativeContentMutation {
        document,
        navigate_to_sources,
        events,
        csp_violations,
        navigation,
        allowed: response
            .get("allowed")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        history,
        scroll_commands,
        storage_events,
        indexed_db_changes,
        dialogs,
        popups,
        post_messages,
        window_closes,
        window_navigations,
        window_name,
    })
}

pub(in crate::native_engine::content_process) fn decode_csp_violations(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeCspViolation>, NativeEngineError> {
    let Some(value) = response.get("csp_violations") else {
        return Ok(Vec::new());
    };
    let violations =
        serde_json::from_value::<Vec<NativeCspViolation>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed CSP violation records".into(),
            }
        })?;
    if violations.len() > MAX_NATIVE_CSP_VIOLATIONS {
        return Err(NativeEngineError::limit(
            "content-process CSP violation records",
            MAX_NATIVE_CSP_VIOLATIONS,
            violations.len(),
        ));
    }
    Ok(violations)
}

pub(in crate::native_engine::content_process) fn decode_scroll_commands(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("scroll_commands") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid scroll commands".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process scroll commands",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let commands =
        serde_json::from_value::<Vec<NativeScriptCommand>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed scroll commands".into(),
            }
        })?;
    for command in &commands {
        let NativeScriptCommand::ScrollTo {
            node_index,
            left,
            top,
        } = command
        else {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned a non-scroll command in scroll_commands".into(),
            });
        };
        if *node_index == u32::MAX || *left < 0 || *top < 0 {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid scroll target or offset".into(),
            });
        }
    }
    Ok(commands)
}

pub(in crate::native_engine::content_process) fn decode_history_commands(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("history") else {
        return Ok(Vec::new());
    };
    decode_history_command_value(value, operation)
}

pub(in crate::native_engine::content_process) fn decode_mutation_history(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("mutation_history") else {
        return Ok(Vec::new());
    };
    decode_history_command_value(value, operation)
}

pub(in crate::native_engine::content_process) fn decode_history_command_value(
    value: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid history commands".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process history commands",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let commands =
        serde_json::from_value::<Vec<NativeScriptCommand>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed history commands".into(),
            }
        })?;
    for command in &commands {
        match command {
            NativeScriptCommand::HistoryPushState { state, .. }
            | NativeScriptCommand::HistoryReplaceState { state, .. } => {
                let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned unserializable history state".into(),
                })?;
                if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
                    return Err(NativeEngineError::limit(
                        "content-process history state",
                        MAX_NATIVE_HISTORY_STATE_BYTES,
                        encoded.len(),
                    ));
                }
            }
            NativeScriptCommand::HistoryGo { delta } => {
                if delta.unsigned_abs() > 1024 {
                    return Err(NativeEngineError::limit(
                        "content-process history traversal delta",
                        1024,
                        delta.unsigned_abs() as usize,
                    ));
                }
            }
            _ => {
                return Err(NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned a non-history command in history output"
                        .into(),
                });
            }
        }
    }
    Ok(commands)
}

pub(in crate::native_engine::content_process) fn decode_event_payload(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeContentEvent>, NativeEngineError> {
    let event_values = response
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process omitted event effects".into(),
        })?;
    if event_values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process mutation effects",
            MAX_NATIVE_EFFECTS,
            event_values.len(),
        ));
    }
    let mut events = Vec::with_capacity(event_values.len());
    for value in event_values {
        let node_index = value
            .get("node_index")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid mutation node".into(),
            })?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(parse_event_kind)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid mutation effect".into(),
            })?;
        events.push(NativeContentEvent { node_index, kind });
    }
    Ok(events)
}

pub(in crate::native_engine::content_process) fn decode_popup_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativePopupRequest>, NativeEngineError> {
    let Some(value) = response.get("popups") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid popup requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process popup requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let mut popups =
        serde_json::from_value::<Vec<NativePopupRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed popup requests".into(),
            }
        })?;
    for popup in &mut popups {
        validate_url_text("content-process popup URL", &popup.url)?;
        if popup.target.is_empty() {
            popup.target = "_blank".into();
        } else {
            validate_url_text("content-process popup target", &popup.target)?;
        }
        validate_target_navigation_payload(
            popup.method,
            popup.body.as_ref(),
            popup.body_content_type.as_deref(),
            "content-process popup navigation payload",
        )?;
        validate_object_url_payload(
            popup.object_url.as_ref(),
            "content-process popup object URL",
        )?;
    }
    Ok(popups)
}

pub(in crate::native_engine::content_process) fn validate_object_url_payload(
    object_url: Option<&NativeObjectUrlResource>,
    operation: &str,
) -> Result<(), NativeEngineError> {
    let Some(object_url) = object_url else {
        return Ok(());
    };
    if object_url.body.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            operation,
            MAX_NATIVE_SCRIPT_BYTES,
            object_url.body.len(),
        ));
    }
    if object_url
        .content_type
        .as_ref()
        .is_some_and(|content_type| content_type.len() > MAX_NATIVE_SCRIPT_BYTES)
    {
        return Err(NativeEngineError::limit(
            operation,
            MAX_NATIVE_SCRIPT_BYTES,
            object_url.content_type.as_ref().map_or(0, String::len),
        ));
    }
    Ok(())
}

pub(in crate::native_engine::content_process) fn decode_post_message_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativePostMessageRequest>, NativeEngineError> {
    let Some(value) = response.get("post_messages") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid postMessage requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process postMessage requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let messages =
        serde_json::from_value::<Vec<NativePostMessageRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed postMessage requests".into(),
            }
        })?;
    for message in &messages {
        if message.target.is_empty() && message.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "content-process postMessage target",
                "must not be empty without a direct target context",
            ));
        }
        if !message.target.is_empty() {
            validate_url_text("content-process postMessage target", &message.target)?;
        }
        validate_url_text(
            "content-process postMessage target origin",
            &message.target_origin,
        )?;
        if let Some(target_context_id) = message.target_context_id.as_deref() {
            validate_context_id(target_context_id)?;
        }
        let encoded = serde_json::to_vec(&message.data).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned unserializable postMessage data".into(),
        })?;
        if encoded.len() > crate::native_engine::javascript::MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "content-process postMessage data",
                crate::native_engine::javascript::MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_object_url_transfers(&message.object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "content-process postMessage data",
        )?;
    }
    Ok(messages)
}

pub(in crate::native_engine::content_process) fn decode_page_message_port_commands(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativePageMessagePortCommand>, NativeEngineError> {
    let Some(value) = response.get("page_message_port_commands") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid page MessagePort commands".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process page MessagePort commands",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let commands = serde_json::from_value::<Vec<NativePageMessagePortCommand>>(value.clone())
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed page MessagePort commands".into(),
        })?;
    for command in &commands {
        crate::native_engine::javascript::validate_page_message_port_command(command)?;
    }
    Ok(commands)
}

pub(in crate::native_engine::content_process) fn decode_shared_worker_commands(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("shared_worker_commands") else {
        return Ok(Vec::new());
    };
    let commands = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid SharedWorker commands".into(),
    })?;
    if commands.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process SharedWorker commands",
            MAX_NATIVE_EFFECTS,
            commands.len(),
        ));
    }
    let commands =
        serde_json::from_value::<Vec<NativeScriptCommand>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed SharedWorker commands".into(),
            }
        })?;
    if commands
        .iter()
        .any(|command| !matches!(command, NativeScriptCommand::SharedWorkerCreate { .. }))
    {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned a non-creation SharedWorker command".into(),
        });
    }
    Ok(commands)
}

pub(in crate::native_engine::content_process) fn defer_shared_worker_commands(
    mut commands: Vec<NativeScriptCommand>,
    external_routing: bool,
    owner_origin: &NativeOrigin,
    owner_context_id: &str,
    owner_frame_id: &str,
    owner_document_generation: u32,
    pending: &mut VecDeque<NativeScriptCommand>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    if !external_routing {
        let storage_key = NativeSharedWorkerStorageKey::for_document(
            owner_origin,
            owner_context_id,
            owner_frame_id,
            owner_document_generation,
        );
        for command in &mut commands {
            if let NativeScriptCommand::SharedWorkerCreate {
                constructor_storage_key,
                ..
            } = command
                && constructor_storage_key.is_none()
            {
                *constructor_storage_key = Some(storage_key.clone());
            }
        }
        return Ok(commands);
    }
    let mut local_commands = Vec::with_capacity(commands.len());
    for mut command in commands {
        if let NativeScriptCommand::SharedWorkerCreate { .. } = &mut command {
            if pending.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "content-process pending SharedWorker commands",
                    MAX_NATIVE_EFFECTS,
                    pending.len().saturating_add(1),
                ));
            }
            pending.push_back(command);
        } else {
            local_commands.push(command);
        }
    }
    Ok(local_commands)
}

pub(in crate::native_engine::content_process) async fn apply_worker_page_message_port_commands(
    workers: &mut NativeWorkerRegistry,
    commands: Vec<NativeScriptCommand>,
    loader: &mut NativeResourceLoader,
    external_shared_worker_routing: bool,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    if external_shared_worker_routing {
        match parent_fetch_broker {
            Some(parent_fetch_broker) => {
                workers
                    .apply_page_message_port_commands_with_external_shared_workers_and_parent_fetch_broker(
                        commands,
                        loader,
                        parent_fetch_broker,
                    )
                    .await
            }
            None => {
                workers
                    .apply_page_message_port_commands_with_external_shared_workers(commands, loader)
                    .await
            }
        }
    } else {
        match parent_fetch_broker {
            Some(parent_fetch_broker) => {
                workers
                    .apply_page_message_port_commands_with_parent_fetch_broker(
                        commands,
                        loader,
                        parent_fetch_broker,
                    )
                    .await
            }
            None => {
                workers
                    .apply_page_message_port_commands(commands, loader)
                    .await
            }
        }
    }
}

pub(in crate::native_engine::content_process) fn decode_content_navigation(
    value: &Value,
    operation: &str,
) -> Result<NativeContentNavigation, NativeEngineError> {
    let node_index = value
        .get("node_index")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid navigation node".into(),
        })?;
    let href =
        value
            .get("href")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid navigation href".into(),
            })?;
    validate_url_text("content process navigation href", href)?;
    let submitter_node_index = match value.get("submitter_node_index") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned an invalid submitter node".into(),
                })?,
        ),
    };
    let raw_target = value
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("_self")
        .trim();
    let target = if raw_target.is_empty() {
        "_self".to_owned()
    } else {
        validate_url_text("content process navigation target", raw_target)?;
        validate_window_name(raw_target)?;
        let lower = raw_target.to_ascii_lowercase();
        match lower.as_str() {
            "_self" | "_parent" | "_top" | "_blank" | "_unfencedtop" => lower,
            _ => raw_target.to_owned(),
        }
    };
    let raw_method = value.get("method").and_then(Value::as_str).unwrap_or("GET");
    let method = NativeNavigationMethod::from_fetch_method(raw_method).map_err(|_| {
        NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an unsupported navigation method".into(),
        }
    })?;
    let body = match value.get("body") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            serde_json::from_value::<NativeRequestBody>(value.clone()).map_err(|_| {
                NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned a malformed navigation body".into(),
                }
            })?,
        ),
    };
    let body_content_type = match value.get("body_content_type") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned a malformed navigation content type".into(),
                })?
                .to_owned(),
        ),
    };
    let object_url = match value.get("object_url") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            serde_json::from_value::<NativeObjectUrlResource>(value.clone()).map_err(|_| {
                NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned a malformed object URL payload".into(),
                }
            })?,
        ),
    };
    if let Some(object_url) = object_url.as_ref() {
        if object_url.body.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process object URL payload",
                MAX_NATIVE_SCRIPT_BYTES,
                object_url.body.len(),
            ));
        }
        if object_url
            .content_type
            .as_ref()
            .is_some_and(|content_type| content_type.len() > MAX_NATIVE_SCRIPT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "content-process object URL content type",
                MAX_NATIVE_SCRIPT_BYTES,
                object_url.content_type.as_ref().map_or(0, String::len),
            ));
        }
    }
    validate_target_navigation_payload(
        method,
        body.as_ref(),
        body_content_type.as_deref(),
        "content process navigation payload",
    )?;
    let location = match value.get("location") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid location navigation flag".into(),
        })?,
    };
    let replace_history = match value.get("replace_history") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid history replacement flag".into(),
        })?,
    };
    if location && (node_index != 0 || submitter_node_index.is_some()) {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "location navigation carried a DOM target".into(),
        });
    }
    if location && method != NativeNavigationMethod::Get {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "location navigation carried a non-GET method".into(),
        });
    }
    if replace_history && !location {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "history replacement was returned without location navigation".into(),
        });
    }
    Ok(NativeContentNavigation {
        node_index,
        href: href.to_owned(),
        submitter_node_index,
        target,
        method,
        body,
        body_content_type,
        location,
        replace_history,
        object_url,
    })
}
