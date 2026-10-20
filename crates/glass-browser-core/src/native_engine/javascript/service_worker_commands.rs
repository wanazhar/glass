use super::*;

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

pub(super) fn is_service_worker_lifecycle_command(command: &NativeScriptCommand) -> bool {
    matches!(
        command,
        NativeScriptCommand::ServiceWorkerSkipWaiting { .. }
            | NativeScriptCommand::ServiceWorkerClientsClaim { .. }
    )
}

pub(super) fn is_service_worker_open_window_command(command: &NativeScriptCommand) -> bool {
    matches!(command, NativeScriptCommand::ServiceWorkerOpenWindow { .. })
}

pub(super) fn validate_service_worker_lifecycle_command(
    command: &NativeScriptCommand,
) -> Result<(), NativeEngineError> {
    let worker_id = match command {
        NativeScriptCommand::ServiceWorkerSkipWaiting { worker_id }
        | NativeScriptCommand::ServiceWorkerClientsClaim { worker_id } => *worker_id,
        _ => return Ok(()),
    };
    if worker_id == 0 {
        return Err(NativeEngineError::invalid(
            "native service worker lifecycle command",
            "worker id must be positive",
        ));
    }
    Ok(())
}

pub(super) fn validate_service_worker_open_window_command(
    command: &NativeScriptCommand,
) -> Result<(), NativeEngineError> {
    let NativeScriptCommand::ServiceWorkerOpenWindow {
        request_id,
        worker_id,
        url,
    } = command
    else {
        return Ok(());
    };
    if *request_id == 0 || *worker_id == 0 {
        return Err(NativeEngineError::invalid(
            "native service worker openWindow command",
            "request and worker ids must be positive",
        ));
    }
    validate_url_text("native service worker openWindow URL", url)?;
    if url.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker openWindow URL",
            MAX_NATIVE_SCRIPT_BYTES,
            url.len(),
        ));
    }
    Ok(())
}

pub(super) fn validate_service_worker_cache_command(
    command: &NativeScriptCommand,
) -> Result<(), NativeEngineError> {
    let request_id = match command {
        NativeScriptCommand::ServiceWorkerCacheOpen { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheDelete { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheHas { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheKeys { request_id }
        | NativeScriptCommand::ServiceWorkerCacheMatch { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCachePut { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCachePutAll { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheEntries { request_id, .. } => *request_id,
        _ => return Ok(()),
    };
    if request_id == 0 {
        return Err(NativeEngineError::invalid(
            "native service worker cache request id",
            "must be positive",
        ));
    }
    let cache_name = match command {
        NativeScriptCommand::ServiceWorkerCacheOpen { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheDelete { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheHas { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheMatch { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCachePut { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCachePutAll { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheEntries { cache_name, .. } => Some(cache_name),
        NativeScriptCommand::ServiceWorkerCacheKeys { .. } => None,
        _ => return Ok(()),
    };
    if let Some(cache_name) = cache_name
        && (cache_name.is_empty() || cache_name.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES)
    {
        return Err(NativeEngineError::limit(
            "native service worker cache name",
            MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES,
            cache_name.len(),
        ));
    }
    match command {
        NativeScriptCommand::ServiceWorkerCacheMatch {
            request_url,
            request_method,
            request_headers,
            ..
        }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest {
            request_url,
            request_method,
            request_headers,
            ..
        } => {
            validate_service_worker_cache_request_fields(
                request_url,
                request_method,
                request_headers,
            )?;
        }
        NativeScriptCommand::ServiceWorkerCachePut {
            request_url,
            request_method,
            request_headers,
            response,
            ..
        } => {
            validate_service_worker_cache_put_fields(
                request_url,
                request_method,
                request_headers,
                response,
            )?;
        }
        NativeScriptCommand::ServiceWorkerCachePutAll { entries, .. } => {
            if entries.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES {
                return Err(NativeEngineError::limit(
                    "native service worker cache batch entries",
                    MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
                    entries.len(),
                ));
            }
            let mut encoded_bytes = 0usize;
            for entry in entries {
                validate_service_worker_cache_put_fields(
                    &entry.request_url,
                    &entry.request_method,
                    &entry.request_headers,
                    &entry.response,
                )?;
                let encoded =
                    serde_json::to_vec(&entry.response).map_err(|_| NativeEngineError::Worker {
                        operation: "validate native service worker cache response".into(),
                        reason: "cached response could not be serialized".into(),
                    })?;
                encoded_bytes = encoded_bytes.saturating_add(encoded.len());
                if encoded_bytes > MAX_NATIVE_SCRIPT_COMMAND_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker cache batch",
                        MAX_NATIVE_SCRIPT_COMMAND_BYTES,
                        encoded_bytes,
                    ));
                }
            }
        }
        NativeScriptCommand::ServiceWorkerCacheEntries {
            request_url,
            request_method,
            request_headers,
            ..
        } => {
            if request_url.is_some() != request_method.is_some() {
                return Err(NativeEngineError::invalid(
                    "native service worker cache keys request",
                    "URL and method must be supplied together",
                ));
            }
            if let Some(request_url) = request_url
                && (request_url.is_empty() || request_url.len() > MAX_NATIVE_SCRIPT_BYTES)
            {
                return Err(NativeEngineError::limit(
                    "native service worker cache request URL",
                    MAX_NATIVE_SCRIPT_BYTES,
                    request_url.len(),
                ));
            }
            if let Some(request_method) = request_method
                && NativeFetchMethod::from_fetch_method(request_method).is_err()
            {
                return Err(NativeEngineError::invalid(
                    "native service worker cache request method",
                    "method is unsupported",
                ));
            }
            validate_native_service_worker_cache_request_headers(request_headers)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_service_worker_cache_put_fields(
    request_url: &str,
    request_method: &str,
    request_headers: &[(String, String)],
    response: &serde_json::Value,
) -> Result<(), NativeEngineError> {
    validate_service_worker_cache_request_fields(request_url, request_method, request_headers)?;
    if request_method != "GET" {
        return Err(NativeEngineError::invalid(
            "native service worker cache request method",
            "Cache.put only supports GET requests",
        ));
    }
    let encoded = serde_json::to_vec(response).map_err(|_| NativeEngineError::Worker {
        operation: "validate native service worker cache response".into(),
        reason: "cached response could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker cache response",
            MAX_NATIVE_SCRIPT_RESULT_BYTES,
            encoded.len(),
        ));
    }
    Ok(())
}

pub(super) fn validate_service_worker_cache_request_fields(
    request_url: &str,
    request_method: &str,
    request_headers: &[(String, String)],
) -> Result<(), NativeEngineError> {
    if request_url.is_empty() || request_url.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker cache request URL",
            MAX_NATIVE_SCRIPT_BYTES,
            request_url.len(),
        ));
    }
    if NativeFetchMethod::from_fetch_method(request_method).is_err() {
        return Err(NativeEngineError::invalid(
            "native service worker cache request method",
            "method is unsupported",
        ));
    }
    validate_native_service_worker_cache_request_headers(request_headers)?;
    Ok(())
}
