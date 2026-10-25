//! Document realm bootstrap generation and bounded host wiring.
//!
//! The JavaScript template is assembled from domain-ordered source assets in
//! `document_bootstrap/`; keep their order aligned with the shared realm scope.

use super::*;

pub(super) struct NativeDocumentBootstrap {
    pub(super) source: String,
    pub(super) host_state_json: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn document_bootstrap(
    document: &NativeDocument,
    document_url: &str,
    document_creation_url: &str,
    context_id: &str,
    window_name: &str,
    opener_context_id: Option<&str>,
    opener_window_name: &str,
    opener_url: &str,
    origin: &NativeOrigin,
    viewport: Viewport,
    scroll_offset: NativePoint,
    nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    history_state: &serde_json::Value,
    history_length: usize,
    ready_state: &str,
    now_ms: u64,
    storage: &NativeWebStorageView,
    indexed_db: &NativeIndexedDbOrigin,
    storage_events: &[NativeStorageEvent],
    cookie: &str,
    frame_bindings: &[NativeFrameScriptBinding],
    frame_context: Option<&NativeFrameScriptContext>,
    service_worker_registrations: &[NativeServiceWorkerRegistrationState],
    run_timers: bool,
    include_layout: bool,
    environment: &NativeEnvironmentOverrides,
    runtime_has_matching_host_state: bool,
) -> Result<NativeDocumentBootstrap, NativeEngineError> {
    let random_seed_stage = crate::native_engine::content_process::ContentLoadStage::begin(
        0,
        "javascript_bootstrap_crypto_seed",
    );
    let native_file_bytes = MAX_NATIVE_FILE_BYTES;
    let native_form_body_bytes = MAX_NATIVE_FORM_BODY_BYTES;
    let javascript_mime_type_essences = JAVASCRIPT_MIME_TYPE_ESSENCES
        .iter()
        .map(|essence| format!("\"{essence}\""))
        .collect::<Vec<_>>()
        .join(",");
    let mut random_bytes = vec![0_u8; MAX_NATIVE_PAGE_CRYPTO_POOL_BYTES];
    getrandom::fill(&mut random_bytes).map_err(|_| NativeEngineError::Worker {
        operation: "seed native page crypto".into(),
        reason: "the operating system did not provide page crypto seed bytes".into(),
    })?;
    random_seed_stage.finish(Some(random_bytes.len()));
    let snapshot_stage = crate::native_engine::content_process::ContentLoadStage::begin(
        0,
        "javascript_bootstrap_dom_snapshot",
    );
    // The persistent QuickJS installer already owns the last successful
    // snapshot. Its runtime key includes document identity/revision, the
    // document's snapshot-invalidation generation, viewport, and scroll
    // projection inputs, so a matching key can reuse that state even when the
    // bounded Rust snapshot cache intentionally declined to retain a copy
    // (for example, because the page has animated image resources).
    let snapshot_is_cached = runtime_has_matching_host_state;
    let reuse_state_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
        crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "javascript_bootstrap_host_state_reuse",
        )
    });
    if let Some(stage) = reuse_state_stage {
        stage.finish(Some(usize::from(snapshot_is_cached)));
    }
    let state = if snapshot_is_cached {
        None
    } else if include_layout {
        Some(document.script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            viewport,
            scroll_offset,
            nested_scroll_offsets,
        )?)
    } else {
        Some(document.script_snapshot_for_viewport(crate::limits::MAX_TEXT_BYTES, viewport))
    };
    snapshot_stage.finish(None);
    let style_dedup_stage = crate::native_engine::content_process::ContentLoadStage::begin(
        0,
        "javascript_bootstrap_style_interning",
    );
    let mut state =
        state
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| NativeEngineError::Worker {
                operation: "compact JavaScript host view".into(),
                reason: "native JavaScript host view could not be compacted".into(),
            })?;
    let mut computed_style_count = 0;
    if let Some(state) = state.as_mut() {
        let mut computed_styles = Vec::new();
        let mut style_indices = std::collections::HashMap::<Vec<u8>, u32>::new();
        if let Some(elements) = state
            .get_mut("elements")
            .and_then(serde_json::Value::as_array_mut)
        {
            for (batch_ordinal, batch) in elements.chunks_mut(128).enumerate() {
                let batch_stage =
                    crate::native_engine::content_process::ContentLoadStage::begin_with_resource(
                        0,
                        "javascript_bootstrap_style_interning_batch",
                        Some(batch_ordinal),
                        Some("elements"),
                    );
                let mut style_key_bytes = 0usize;
                for element in batch {
                    let Some(element) = element.as_object_mut() else {
                        continue;
                    };
                    let computed_style = element
                        .remove("computedStyle")
                        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
                    let style_key = serde_json::to_vec(&computed_style).map_err(|_| {
                        NativeEngineError::Worker {
                            operation: "compact JavaScript computed style".into(),
                            reason: "native computed style could not be compacted".into(),
                        }
                    })?;
                    style_key_bytes = style_key_bytes.saturating_add(style_key.len());
                    let style_index = if let Some(index) = style_indices.get(&style_key) {
                        *index
                    } else {
                        let index = u32::try_from(computed_styles.len()).map_err(|_| {
                            NativeEngineError::limit(
                                "native computed style table",
                                u32::MAX as usize,
                                computed_styles.len(),
                            )
                        })?;
                        computed_styles.push(computed_style);
                        style_indices.insert(style_key, index);
                        index
                    };
                    // This key is repeated for every styled element in the
                    // 16 MiB-bounded host snapshot. Keep the transport name
                    // compact; runtime_timers.js restores the computed style
                    // from the shared table before page scripts run.
                    element.insert("cs".into(), serde_json::json!(style_index));
                }
                batch_stage.finish(Some(style_key_bytes));
            }
        }
        computed_style_count = computed_styles.len();
        if let Some(state) = state.as_object_mut() {
            state.insert(
                "computedStyles".into(),
                serde_json::Value::Array(computed_styles),
            );
        }
    }
    style_dedup_stage.finish(Some(computed_style_count));
    let serialization_stage = crate::native_engine::content_process::ContentLoadStage::begin(
        0,
        "javascript_bootstrap_host_serialize",
    );
    let serialized = serde_json::to_string(&serde_json::json!({
        "url": document_url,
        "base_url": document.effective_base_url(document_url),
        "creation_url": document_creation_url,
        "document_referrer_policy": document.document_referrer_policy().as_str(),
        "compat_mode": document.compat_mode(),
        "context_id": context_id,
        "frame_id": context_id,
        "generation": document.generation(),
        "window_name": window_name,
        "opener_context_id": opener_context_id,
        "opener_window_name": opener_window_name,
        "opener_url": opener_url,
        "origin": origin.serialized(),
        "state": state,
        "reuse_state": snapshot_is_cached,
        "font_faces": document.font_face_script_descriptors(),
        "history_state": history_state,
        "history_length": history_length,
        "now_ms": now_ms,
        "storage": storage,
        "indexed_db": indexed_db,
        "storage_events": storage_events,
        "cookie": cookie,
        "frames": frame_bindings,
        "frame_context": frame_context,
        "service_workers": service_worker_registrations,
        "device_scale_factor_milli": viewport.device_scale_factor_milli,
        "user_agent": environment.user_agent(),
        "accept_language": environment.accept_language(),
        "platform": environment.platform(),
        "geolocation": &environment.geolocation,
        "timezone_id": &environment.timezone_id,
        "cpu_throttling_rate": environment.cpu_throttling_rate,
        "__glassBootstrap": {
            "width": viewport.width,
            "height": viewport.height,
            "readyState": ready_state,
            "runTimers": run_timers,
            "initialPageCryptoBytes": random_bytes,
        },
    }))
    .map_err(|_| NativeEngineError::Worker {
        operation: "serialize JavaScript host view".into(),
        reason: "native JavaScript host view could not be serialized".into(),
    })?;
    serialization_stage.finish(Some(serialized.len()));
    // Pass the large snapshot into QuickJS as a runtime string. Embedding it
    // in the generated JavaScript makes the parser retain a large string
    // literal and its syntax tree at the same time as JSON.parse allocates the
    // object graph. The evaluator deletes this temporary global after parsing.
    let message_channel_script = message_channel_bootstrap();
    let service_worker_page_script = service_worker_page_script();
    let xml_document_script = native_xml_document_script();
    let font_face_script = native_font_face_script();
    let native_dom_max_depth = MAX_NATIVE_DOM_DEPTH;
    let native_dom_max_nodes = MAX_NATIVE_NODES;
    let assembly_stage = crate::native_engine::content_process::ContentLoadStage::begin(
        0,
        "javascript_bootstrap_template_assembly",
    );
    let bootstrap = format!(
        concat!(
            include_str!("document_bootstrap/runtime_timers.js"),
            include_str!("document_bootstrap/storage_indexeddb.js"),
            include_str!("document_bootstrap/fetch_requests.js"),
            include_str!("document_bootstrap/fetch_streams.js"),
            include_str!("document_bootstrap/response_xhr.js"),
            include_str!("document_bootstrap/websocket_eventsource.js"),
            include_str!("document_bootstrap/fetch_timer_dispatch.js"),
            include_str!("document_bootstrap/event_listeners.js"),
            include_str!("document_bootstrap/event_objects_and_encoding.js"),
            include_str!("document_bootstrap/mutation_observers.js"),
            include_str!("document_bootstrap/validity_and_html_fragment.js"),
            include_str!("document_bootstrap/attribute_nodes.js"),
            include_str!("document_bootstrap/native_elements.js"),
            include_str!("document_bootstrap/detached_nodes_and_snapshots.js"),
            include_str!("document_bootstrap/node_sync_collections_clone.js"),
            include_str!("document_bootstrap/selectors_and_styles.js"),
            include_str!("document_bootstrap/document_apis.js"),
            include_str!("document_bootstrap/custom_elements.js"),
            include_str!("document_bootstrap/frame_routing.js"),
            include_str!("document_bootstrap/frame_documents.js"),
            include_str!("document_bootstrap/window_events.js"),
        ),
        native_file_bytes = native_file_bytes,
        native_form_body_bytes = native_form_body_bytes,
        javascript_mime_type_essences = javascript_mime_type_essences,
        native_dom_max_nodes = native_dom_max_nodes,
        native_dom_max_depth = native_dom_max_depth,
        font_face_script = font_face_script,
        max_commands = super::super::interaction::MAX_NATIVE_EFFECTS,
        max_listeners = super::super::interaction::MAX_NATIVE_EFFECTS,
        max_timers = super::super::interaction::MAX_NATIVE_EFFECTS,
        storage_entry_limit = crate::limits::MAX_STORAGE_ENTRIES,
        storage_key_limit = crate::limits::MAX_BACKEND_ID_BYTES,
        storage_value_limit = crate::limits::MAX_TEXT_BYTES,
        indexed_db_database_limit = MAX_NATIVE_INDEXED_DB_DATABASES,
        indexed_db_store_limit = MAX_NATIVE_INDEXED_DB_STORES,
        indexed_db_index_limit = MAX_NATIVE_INDEXED_DB_INDEXES,
        indexed_db_record_limit = MAX_NATIVE_INDEXED_DB_RECORDS,
        indexed_db_value_limit = MAX_NATIVE_INDEXED_DB_VALUE_BYTES,
        storage_quota = MAX_WEB_STORAGE_PROFILE_BYTES,
        fetch_header_count_limit = MAX_NATIVE_FETCH_HEADERS,
        fetch_header_name_limit = MAX_NATIVE_FETCH_HEADER_NAME_BYTES,
        fetch_header_value_limit = MAX_NATIVE_FETCH_HEADER_VALUE_BYTES,
        fetch_header_bytes_limit = MAX_NATIVE_FETCH_HEADER_BYTES,
        fetch_method_limit = super::super::resource_loader::MAX_NATIVE_FETCH_METHOD_BYTES,
        max_native_xhr_timeout_ms = MAX_NATIVE_XHR_TIMEOUT_MS,
        websocket_message_limit = MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
        websocket_protocol_limit = MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES,
        websocket_protocol_count_limit = MAX_NATIVE_WEBSOCKET_PROTOCOLS,
        websocket_close_reason_limit = MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
        eventsource_message_limit = MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES,
        eventsource_field_limit = MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
        fetch_stream_chunk_limit = MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES,
        fetch_stream_body_limit = MAX_NATIVE_FETCH_STREAM_BODY_BYTES,
        fetch_stream_queue_limit = MAX_NATIVE_FETCH_STREAM_QUEUED_CHUNKS,
        dialog_text_limit = MAX_NATIVE_DIALOG_TEXT_BYTES,
        max_frame_window_indices = MAX_NATIVE_FRAME_SCRIPT_BINDINGS,
        window_name_bytes_limit = MAX_NATIVE_WINDOW_NAME_BYTES,
        history_state_bytes_limit = MAX_NATIVE_HISTORY_STATE_BYTES,
        history_length_limit = MAX_NATIVE_HISTORY_DELTA,
        page_crypto_pool_limit = MAX_NATIVE_PAGE_CRYPTO_POOL_BYTES,
        page_crypto_values_limit = MAX_NATIVE_PAGE_CRYPTO_VALUES_BYTES,
        native_crypto_derive_work = MAX_NATIVE_CRYPTO_DERIVE_WORK,
        native_canvas_source = NATIVE_CANVAS_SCRIPT,
        message_channel_script = message_channel_script,
        service_worker_page_script = service_worker_page_script,
        xml_document_script = xml_document_script,
        dynamic_import_bootstrap = NATIVE_DYNAMIC_IMPORT_OPTIONS_BOOTSTRAP,
    );
    assembly_stage.finish(Some(bootstrap.len()));
    Ok(NativeDocumentBootstrap {
        source: bootstrap,
        host_state_json: serialized,
    })
}
