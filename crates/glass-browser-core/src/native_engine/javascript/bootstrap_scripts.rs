use super::*;

pub(super) fn service_worker_bootstrap(
    worker_id: u32,
    worker_url: &str,
    now_ms: u64,
    import_script_counts: &BTreeMap<String, usize>,
    is_module: bool,
    worker_global_referrer_policy: Option<NativeFetchReferrerPolicy>,
) -> Result<String, NativeEngineError> {
    let mut bootstrap = worker_bootstrap(
        worker_id,
        worker_url,
        now_ms,
        import_script_counts,
        is_module,
        worker_global_referrer_policy,
        WorkerBootstrapCapabilities {
            report_message_callback_errors: true,
            forward_message_callback_errors: false,
            capture_connect_callback_errors: false,
        },
    )?;
    let marker = "})()";
    let insertion = bootstrap
        .rfind(marker)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "install native service worker host view".into(),
            reason: "native service worker bootstrap boundary was not found".into(),
        })?;
    bootstrap.insert_str(insertion, &service_worker_bootstrap_script());
    Ok(bootstrap)
}

pub(super) fn shared_worker_bootstrap(
    worker_id: u32,
    worker_url: &str,
    now_ms: u64,
    import_script_counts: &BTreeMap<String, usize>,
    is_module: bool,
    worker_global_referrer_policy: Option<NativeFetchReferrerPolicy>,
) -> Result<String, NativeEngineError> {
    let mut bootstrap = worker_bootstrap(
        worker_id,
        worker_url,
        now_ms,
        import_script_counts,
        is_module,
        worker_global_referrer_policy,
        WorkerBootstrapCapabilities {
            report_message_callback_errors: false,
            forward_message_callback_errors: false,
            capture_connect_callback_errors: true,
        },
    )?;
    let marker = "})()";
    let insertion = bootstrap
        .rfind(marker)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "install native SharedWorker host view".into(),
            reason: "native SharedWorker bootstrap boundary was not found".into(),
        })?;
    bootstrap.insert_str(insertion, NATIVE_SHARED_WORKER_BOOTSTRAP);
    Ok(bootstrap)
}

pub(super) fn service_worker_bootstrap_script() -> String {
    NATIVE_SERVICE_WORKER_BOOTSTRAP
        .replace(
            "__GLASS_SERVICE_WORKER_BODY_LIMIT__",
            &MAX_NATIVE_FORM_BODY_BYTES.to_string(),
        )
        .replace(
            "__GLASS_SERVICE_WORKER_CACHE_NAME_LIMIT__",
            &MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES.to_string(),
        )
        .replace(
            "__GLASS_SERVICE_WORKER_CACHE_ENTRY_LIMIT__",
            &MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES.to_string(),
        )
        .replace(
            "__GLASS_SERVICE_WORKER_PRELOAD_LIMIT__",
            &MAX_NATIVE_WORKER_MESSAGES.to_string(),
        )
}

pub(super) fn is_ready_state_comparison(source: &str) -> bool {
    let source = source
        .trim()
        .strip_suffix(';')
        .unwrap_or(source.trim())
        .trim();
    for operator in ["===", "!==", "==", "!="] {
        let Some((left, right)) = source.split_once(operator) else {
            continue;
        };
        let right = right.trim();
        let is_string_literal = right.len() >= 2
            && matches!(right.as_bytes().first(), Some(b'\'' | b'"'))
            && right.as_bytes().first() == right.as_bytes().last();
        return left.trim() == "document.readyState" && is_string_literal;
    }
    false
}

pub(super) fn message_channel_bootstrap() -> String {
    NATIVE_MESSAGE_CHANNEL_BOOTSTRAP
        .replace(
            "__GLASS_MESSAGE_PORT_QUEUE_LIMIT__",
            &super::super::interaction::MAX_NATIVE_EFFECTS.to_string(),
        )
        .replace(
            "__GLASS_MESSAGE_CHANNEL_NAME_LIMIT__",
            &crate::limits::MAX_BACKEND_ID_BYTES.to_string(),
        )
        .replace(
            "__GLASS_MESSAGE_BYTES_LIMIT__",
            &MAX_NATIVE_POST_MESSAGE_BYTES.to_string(),
        )
}

pub(super) fn service_worker_page_script() -> String {
    NATIVE_SERVICE_WORKER_PAGE_SCRIPT
        .replace(
            "__GLASS_SERVICE_WORKER_MESSAGE_LIMIT__",
            &MAX_NATIVE_POST_MESSAGE_BYTES.to_string(),
        )
        .replace(
            "__GLASS_SERVICE_WORKER_NAVIGATION_PRELOAD_HEADER_VALUE_LIMIT__",
            &MAX_NATIVE_FETCH_HEADER_VALUE_BYTES.to_string(),
        )
}

pub(super) fn native_xml_document_script() -> String {
    NATIVE_XML_DOCUMENT_SCRIPT
        .replace(
            "__GLASS_XML_MAX_BYTES__",
            &crate::limits::MAX_TEXT_BYTES.to_string(),
        )
        .replace(
            "__GLASS_HTML_MAX_BYTES__",
            &crate::limits::MAX_TEXT_BYTES.saturating_mul(3).to_string(),
        )
        .replace(
            "__GLASS_XML_MAX_NODES__",
            &super::super::config::MAX_NATIVE_NODES.to_string(),
        )
        .replace(
            "__GLASS_XML_MAX_DEPTH__",
            &super::super::config::MAX_NATIVE_DOM_DEPTH.to_string(),
        )
}
