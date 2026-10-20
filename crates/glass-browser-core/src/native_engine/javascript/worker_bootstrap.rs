//! Worker realm bootstrap assembly and host capability wiring.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct WorkerBootstrapCapabilities {
    pub(super) report_message_callback_errors: bool,
    pub(super) forward_message_callback_errors: bool,
    pub(super) capture_connect_callback_errors: bool,
}

pub(super) fn worker_bootstrap(
    worker_id: u32,
    worker_url: &str,
    now_ms: u64,
    import_script_counts: &BTreeMap<String, usize>,
    is_module: bool,
    worker_global_referrer_policy: Option<NativeFetchReferrerPolicy>,
    capabilities: WorkerBootstrapCapabilities,
) -> Result<String, NativeEngineError> {
    let WorkerBootstrapCapabilities {
        report_message_callback_errors,
        forward_message_callback_errors,
        capture_connect_callback_errors,
    } = capabilities;
    let worker_url = serde_json::to_string(worker_url).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native Worker URL".into(),
        reason: "native Worker URL could not be serialized".into(),
    })?;
    let import_script_counts =
        serde_json::to_string(import_script_counts).map_err(|_| NativeEngineError::Worker {
            operation: "serialize native Worker importScripts map".into(),
            reason: "native Worker importScripts map could not be serialized".into(),
        })?;
    let worker_global_referrer_policy = serde_json::to_string(
        worker_global_referrer_policy.map_or("", NativeFetchReferrerPolicy::as_str),
    )
    .map_err(|_| NativeEngineError::Worker {
        operation: "serialize native Worker referrer policy".into(),
        reason: "native Worker referrer policy could not be serialized".into(),
    })?;
    let mut random_bytes = vec![0_u8; MAX_NATIVE_WORKER_CRYPTO_POOL_BYTES];
    getrandom::fill(&mut random_bytes).map_err(|_| NativeEngineError::Worker {
        operation: "seed native Worker crypto".into(),
        reason: "the operating system random source was unavailable".into(),
    })?;
    let initial_random_bytes =
        serde_json::to_string(&random_bytes).map_err(|_| NativeEngineError::Worker {
            operation: "serialize native Worker crypto seed".into(),
            reason: "native Worker crypto seed could not be serialized".into(),
        })?;
    let message_channel_script = message_channel_bootstrap();
    let is_module = if is_module { "true" } else { "false" };
    let report_message_callback_errors = if report_message_callback_errors {
        "true"
    } else {
        "false"
    };
    let forward_message_callback_errors = if forward_message_callback_errors {
        "true"
    } else {
        "false"
    };
    let capture_connect_callback_errors = if capture_connect_callback_errors {
        "true"
    } else {
        "false"
    };
    let mut bootstrap = [
        include_str!("worker_bootstrap/realm.js"),
        include_str!("worker_bootstrap/url_location.js"),
        include_str!("worker_bootstrap/abort_and_script_imports.js"),
        include_str!("worker_bootstrap/request_and_event_primitives.js"),
        include_str!("worker_bootstrap/crypto.js"),
        include_str!("worker_bootstrap/blob_and_headers.js"),
        include_str!("worker_bootstrap/readable_writable_streams.js"),
        include_str!("worker_bootstrap/fetch_and_xhr.js"),
        include_str!("worker_bootstrap/websocket_and_eventsource.js"),
        include_str!("worker_bootstrap/worker_control.js"),
    ]
    .concat();
    let worker_id = worker_id.to_string();
    let now_ms = now_ms.to_string();
    let replacements = [
        (
            "__GLASS_WORKER_CAPTURE_CONNECT_CALLBACK_ERRORS__",
            capture_connect_callback_errors.to_owned(),
        ),
        (
            "__GLASS_WORKER_EVENTSOURCE_FIELD_LIMIT__",
            MAX_NATIVE_EVENTSOURCE_FIELD_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_EVENTSOURCE_MESSAGE_LIMIT__",
            MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_BODY_LIMIT__",
            MAX_NATIVE_FORM_BODY_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_HEADER_BYTES_LIMIT__",
            MAX_NATIVE_FETCH_HEADER_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_HEADER_COUNT_LIMIT__",
            MAX_NATIVE_FETCH_HEADERS.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_HEADER_NAME_LIMIT__",
            MAX_NATIVE_FETCH_HEADER_NAME_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_HEADER_VALUE_LIMIT__",
            MAX_NATIVE_FETCH_HEADER_VALUE_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_METHOD_LIMIT__",
            super::super::resource_loader::MAX_NATIVE_FETCH_METHOD_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_STREAM_BODY_LIMIT__",
            MAX_NATIVE_FETCH_STREAM_BODY_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_STREAM_CHUNK_LIMIT__",
            MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__",
            MAX_NATIVE_FETCH_STREAM_QUEUED_CHUNKS.to_string(),
        ),
        (
            "__GLASS_WORKER_FORWARD_MESSAGE_CALLBACK_ERRORS__",
            forward_message_callback_errors.to_owned(),
        ),
        (
            "__GLASS_WORKER_IMPORT_SCRIPT_COUNTS__",
            import_script_counts,
        ),
        (
            "__GLASS_WORKER_INITIAL_RANDOM_BYTES__",
            initial_random_bytes,
        ),
        ("__GLASS_WORKER_IS_MODULE__", is_module.to_owned()),
        (
            "__GLASS_WORKER_MAX_COMMANDS__",
            MAX_NATIVE_WORKER_MESSAGES.to_string(),
        ),
        (
            "__GLASS_WORKER_MAX_NATIVE_XHR_TIMEOUT_MS__",
            MAX_NATIVE_XHR_TIMEOUT_MS.to_string(),
        ),
        (
            "__GLASS_WORKER_MAX_TIMERS__",
            MAX_NATIVE_WORKER_TIMERS.to_string(),
        ),
        (
            "__GLASS_WORKER_MESSAGE_CHANNEL_SCRIPT__",
            message_channel_script,
        ),
        (
            "__GLASS_WORKER_NATIVE_CRYPTO_DERIVE_WORK__",
            MAX_NATIVE_CRYPTO_DERIVE_WORK.to_string(),
        ),
        ("__GLASS_WORKER_NOW_MS__", now_ms),
        (
            "__GLASS_WORKER_REPORT_MESSAGE_CALLBACK_ERRORS__",
            report_message_callback_errors.to_owned(),
        ),
        (
            "__GLASS_WORKER_WEBSOCKET_CLOSE_REASON_LIMIT__",
            MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_WEBSOCKET_MESSAGE_LIMIT__",
            MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_WEBSOCKET_PROTOCOL_COUNT_LIMIT__",
            MAX_NATIVE_WEBSOCKET_PROTOCOLS.to_string(),
        ),
        (
            "__GLASS_WORKER_WEBSOCKET_PROTOCOL_LIMIT__",
            MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_WORKER_CRYPTO_POOL_LIMIT__",
            MAX_NATIVE_WORKER_CRYPTO_POOL_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_WORKER_CRYPTO_VALUES_LIMIT__",
            MAX_NATIVE_WORKER_CRYPTO_VALUES_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_WORKER_GLOBAL_REFERRER_POLICY__",
            worker_global_referrer_policy,
        ),
        ("__GLASS_WORKER_WORKER_ID__", worker_id),
        ("__GLASS_WORKER_WORKER_URL__", worker_url),
        (
            "__GLASS_WORKER_WORKER_URL_SEARCH_PARAMS_BYTES_LIMIT__",
            MAX_NATIVE_WORKER_URLSEARCHPARAMS_BYTES.to_string(),
        ),
        (
            "__GLASS_WORKER_WORKER_URL_SEARCH_PARAMS_ENTRIES_LIMIT__",
            MAX_NATIVE_WORKER_URLSEARCHPARAMS_ENTRIES.to_string(),
        ),
    ];
    for (placeholder, value) in replacements {
        bootstrap = bootstrap.replace(placeholder, &value);
    }
    bootstrap.push_str(NATIVE_DYNAMIC_IMPORT_OPTIONS_BOOTSTRAP);
    Ok(bootstrap)
}
