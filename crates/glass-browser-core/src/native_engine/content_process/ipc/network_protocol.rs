use super::*;

pub(in crate::native_engine::content_process) fn decode_parent_fetch_request<'a>(
    document_url: &'a str,
    href: &'a str,
    credentials: bool,
    value: &Value,
) -> Result<NativeFetchRequest<'a>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent Fetch request",
            NativeWorkerFailureKind::Protocol,
            "Fetch request metadata must be an object",
        )
    })?;
    let required_text = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch request",
                    NativeWorkerFailureKind::Protocol,
                    format!("Fetch request omitted {name}"),
                )
            })
    };
    let optional_text = |name: &str| -> Result<Option<String>, NativeEngineError> {
        match object.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_str()
                .map(|value| Some(value.to_owned()))
                .ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "decode parent Fetch request",
                        NativeWorkerFailureKind::Protocol,
                        format!("Fetch request returned invalid {name}"),
                    )
                }),
        }
    };

    let method = NativeFetchMethod::from_fetch_method(&required_text("method")?)?;
    let body = object
        .get("body")
        .filter(|value| !value.is_null())
        .map(|value| {
            serde_json::from_value::<NativeRequestBody>(value.clone()).map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch request",
                    NativeWorkerFailureKind::Protocol,
                    "Fetch request body is malformed",
                )
            })
        })
        .transpose()?;
    let content_type = optional_text("content_type")?;
    let request_headers = object
        .get("request_headers")
        .map(|value| serde_json::from_value::<BTreeMap<String, String>>(value.clone()))
        .transpose()
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent Fetch request",
                NativeWorkerFailureKind::Protocol,
                "Fetch request headers are malformed",
            )
        })?
        .unwrap_or_default();
    if request_headers
        .keys()
        .any(|name| matches!(name.to_ascii_lowercase().as_str(), "cookie" | "cookie2"))
    {
        return Err(NativeEngineError::worker_failure(
            "decode parent Fetch request",
            NativeWorkerFailureKind::Protocol,
            "child Fetch request must not carry cookie headers",
        ));
    }
    let cors_mode = match required_text("cors_mode")?.as_str() {
        "cors" => NativeCorsMode::Cors,
        "no-cors" => NativeCorsMode::NoCors,
        "same-origin" => NativeCorsMode::SameOrigin,
        "navigation" => NativeCorsMode::Navigation,
        _ => {
            return Err(NativeEngineError::worker_failure(
                "decode parent Fetch request",
                NativeWorkerFailureKind::Protocol,
                "Fetch request has an unsupported CORS mode",
            ));
        }
    };
    let redirect_mode = match required_text("redirect_mode")?.as_str() {
        "follow" => NativeFetchRedirectMode::Follow,
        "error" => NativeFetchRedirectMode::Error,
        "manual" => NativeFetchRedirectMode::Manual,
        _ => {
            return Err(NativeEngineError::worker_failure(
                "decode parent Fetch request",
                NativeWorkerFailureKind::Protocol,
                "Fetch request has an unsupported redirect mode",
            ));
        }
    };
    let cache_mode = NativeFetchCacheMode::from_option(Some(&required_text("cache_mode")?))?;
    let credentials_mode = optional_text("credentials_mode")?
        .as_deref()
        .map(NativeFetchCredentialsMode::parse)
        .transpose()?;
    if credentials_mode.is_some_and(|mode| match mode {
        NativeFetchCredentialsMode::Omit => credentials,
        NativeFetchCredentialsMode::SameOrigin => false,
        NativeFetchCredentialsMode::Include => !credentials,
    }) {
        return Err(NativeEngineError::worker_failure(
            "decode parent Fetch request",
            NativeWorkerFailureKind::Protocol,
            "Fetch credentials flag disagrees with its credentials mode",
        ));
    }
    let timeout = match object.get("timeout_ms") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let timeout_ms = value.as_u64().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch request",
                    NativeWorkerFailureKind::Protocol,
                    "Fetch timeout is malformed",
                )
            })?;
            if timeout_ms > u64::from(MAX_NATIVE_XHR_TIMEOUT_MS) {
                return Err(NativeEngineError::limit(
                    "parent Fetch timeout",
                    usize::try_from(MAX_NATIVE_XHR_TIMEOUT_MS).unwrap_or(usize::MAX),
                    usize::try_from(timeout_ms).unwrap_or(usize::MAX),
                ));
            }
            Some(Duration::from_millis(timeout_ms))
        }
    };
    let referrer_url = optional_text("referrer_url")?;
    let referrer_policy = optional_text("referrer_policy")?;
    if let Some(policy) = referrer_policy.as_deref() {
        NativeFetchReferrerPolicy::parse(policy)?;
    }

    let max_response_bytes = match object.get("max_response_bytes") {
        None | Some(Value::Null) => MAX_CONTENT_DOCUMENT_WIRE_BYTES,
        Some(value) => {
            let max_response_bytes = value.as_u64().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch request",
                    NativeWorkerFailureKind::Protocol,
                    "Fetch response limit is malformed",
                )
            })?;
            usize::try_from(max_response_bytes).unwrap_or(usize::MAX)
        }
    };

    Ok(NativeFetchRequest {
        document_url,
        href,
        referrer_url,
        referrer_policy,
        method,
        body,
        content_type,
        request_headers,
        credentials,
        credentials_mode,
        cors_mode,
        redirect_mode,
        cache_mode,
        timeout,
        max_response_bytes: Some(max_response_bytes.min(MAX_CONTENT_DOCUMENT_WIRE_BYTES)),
    })
}

pub(in crate::native_engine::content_process) fn decode_content_cookie_owner(
    value: &Value,
    operation: &'static str,
) -> Result<NativeContentCookieOwner, NativeEngineError> {
    let owner: NativeContentCookieOwner = serde_json::from_value(value.clone()).map_err(|_| {
        NativeEngineError::worker_failure(
            operation,
            NativeWorkerFailureKind::Protocol,
            "cookie owner identity is malformed",
        )
    })?;
    validate_context_id(&owner.context_id).map_err(|_| {
        NativeEngineError::worker_failure(
            operation,
            NativeWorkerFailureKind::Protocol,
            "cookie owner context identity is invalid",
        )
    })?;
    validate_context_id(&owner.frame_id).map_err(|_| {
        NativeEngineError::worker_failure(
            operation,
            NativeWorkerFailureKind::Protocol,
            "cookie owner frame identity is invalid",
        )
    })?;
    if owner.generation == 0 {
        return Err(NativeEngineError::worker_failure(
            operation,
            NativeWorkerFailureKind::Protocol,
            "cookie owner generation must be positive",
        ));
    }
    validate_url_text("cookie owner document URL", &owner.document_url).map_err(|_| {
        NativeEngineError::worker_failure(
            operation,
            NativeWorkerFailureKind::Protocol,
            "cookie owner document URL is invalid",
        )
    })?;
    Ok(owner)
}

pub(in crate::native_engine::content_process) fn decode_parent_page_meta_csp(
    value: &Value,
) -> Result<Vec<String>, NativeEngineError> {
    let policies = serde_json::from_value::<Vec<String>>(value.clone()).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent page CSP metadata",
            NativeWorkerFailureKind::Protocol,
            "page resource request carries malformed CSP metadata",
        )
    })?;
    if policies.len() > MAX_NATIVE_CSP_POLICIES {
        return Err(NativeEngineError::limit(
            "parent page CSP metadata",
            MAX_NATIVE_CSP_POLICIES,
            policies.len(),
        ));
    }
    let mut total_bytes = 0usize;
    for policy in &policies {
        if policy.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "parent page CSP policy",
                MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                policy.len(),
            ));
        }
        total_bytes = total_bytes.saturating_add(policy.len());
    }
    if total_bytes > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "parent page CSP metadata",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            total_bytes,
        ));
    }
    Ok(policies)
}

pub(in crate::native_engine::content_process) fn parent_fetch_response_payload(
    id: u64,
    result: Result<NativeFetchResponse, NativeEngineError>,
) -> Value {
    match result {
        Ok(response) => json!({
            "kind": "fetched",
            "id": id,
            "url": response.url,
            "status": response.status,
            "status_text": response.status_text,
            "content_type": response.content_type,
            "headers": response.headers,
            "redirected": response.redirected,
            "opaque": response.opaque,
            "opaque_redirect": response.opaque_redirect,
            "body_base64": base64::engine::general_purpose::STANDARD.encode(response.body),
        }),
        Err(error) => {
            let timeout = matches!(
                &error,
                NativeEngineError::Network { reason, .. }
                    if reason.ends_with("request timed out")
            );
            let mut response = content_error_response(Value::from(id), error);
            response["timeout"] = Value::Bool(timeout);
            response
        }
    }
}

pub(in crate::native_engine::content_process) async fn load_parent_worker_script_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeScriptResource>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "worker script metadata must be an object",
        )
    })?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "referrer_url"
                | "max_source_bytes"
                | "module_type"
                | "credentials_mode"
                | "referrer_policy"
                | "shared_worker_module_entry"
                | "entry_referrer_url"
        )
    }) {
        return Err(NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "worker script metadata contains an unknown field",
        ));
    }
    let optional_text = |name: &str| -> Result<Option<String>, NativeEngineError> {
        match object.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_str().map(str::to_owned).map(Some).ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent worker script request",
                    NativeWorkerFailureKind::Protocol,
                    format!("worker script metadata has an invalid {name}"),
                )
            }),
        }
    };
    let referrer_url = optional_text("referrer_url")?;
    let entry_referrer_url = optional_text("entry_referrer_url")?;
    let credentials_mode = optional_text("credentials_mode")?;
    let referrer_policy = optional_text("referrer_policy")?
        .as_deref()
        .map(NativeFetchReferrerPolicy::parse)
        .transpose()?;
    let module_type = optional_text("module_type")?
        .map(|module_type| match module_type.as_str() {
            "javascript" => Ok(NativeModuleResourceType::JavaScript),
            "json" => Ok(NativeModuleResourceType::Json),
            "unsupported" => Ok(NativeModuleResourceType::Unsupported),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "worker script metadata has an unsupported module type",
            )),
        })
        .transpose()?;
    let max_source_bytes = object
        .get("max_source_bytes")
        .and_then(Value::as_u64)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes > 0 && *bytes <= MAX_NATIVE_SCRIPT_BYTES)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "worker script source limit is invalid",
            )
        })?;
    let shared_worker_module_entry = object
        .get("shared_worker_module_entry")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "worker script metadata omitted its request type",
            )
        })?;
    if entry_referrer_url.is_some()
        && (referrer_url.is_some()
            || module_type.is_some()
            || credentials_mode.is_some()
            || shared_worker_module_entry)
    {
        return Err(NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "worker entry referrer metadata is inconsistent",
        ));
    }
    if credentials_mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "omit" | "same-origin" | "include"))
    {
        return Err(NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "worker script credentials mode is invalid",
        ));
    }
    if module_type == Some(NativeModuleResourceType::Unsupported) {
        return Err(NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "worker script request has an unsupported module type",
        ));
    }
    let request_base = referrer_url.as_deref().unwrap_or(document_url);
    validate_url_text("worker script request referrer URL", request_base)?;
    let request_base = url::Url::parse(without_fragment(request_base)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "worker script request referrer URL is invalid",
        )
    })?;
    validate_url_text("parent worker script target URL", href)?;
    let target = url::Url::parse(href)
        .or_else(|_| request_base.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "worker script target URL is invalid",
            )
        })?;
    if !is_network_url(target.as_str()) {
        return Err(NativeEngineError::worker_failure(
            "decode parent worker script request",
            NativeWorkerFailureKind::Protocol,
            "parent worker script broker accepts only HTTP(S) targets",
        ));
    }

    if let Some(entry_referrer_url) = entry_referrer_url.as_deref() {
        validate_url_text("worker entry referrer URL", entry_referrer_url)?;
        let entry_referrer =
            url::Url::parse(without_fragment(entry_referrer_url)).map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent worker script request",
                    NativeWorkerFailureKind::Protocol,
                    "worker entry referrer URL is invalid",
                )
            })?;
        let owner_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "worker entry owner URL is invalid",
            )
        })?;
        if !is_network_url(entry_referrer.as_str()) || entry_referrer.origin() != owner_url.origin()
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "worker entry referrer must be same-origin with its owner",
            ));
        }
    }

    if shared_worker_module_entry {
        if referrer_url.is_some() || module_type.is_some() || credentials_mode.is_none() {
            return Err(NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "SharedWorker module entry metadata is inconsistent",
            ));
        }
        let Some(credentials_mode) = credentials_mode.as_deref() else {
            return Err(NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "SharedWorker module entry omitted its credentials mode",
            ));
        };
        loader
            .load_shared_worker_module_async(
                document_url,
                href,
                max_source_bytes,
                credentials_mode,
                referrer_policy,
            )
            .await
    } else if let Some(referrer_url) = referrer_url.as_deref() {
        loader
            .load_worker_script_dependency_async_with_referrer_source(NativeScriptLoadRequest {
                document_url,
                referrer_url: Some(referrer_url),
                href,
                max_source_bytes,
                subresource_kind: NativeSubresourceKind::Script,
                parser_inserted: true,
                nonce: None,
                integrity: None,
                crossorigin: None,
                object_url: None,
                module_type,
                worker_credentials_mode: credentials_mode.as_deref(),
                referrer_policy,
            })
            .await
    } else if let Some(entry_referrer_url) = entry_referrer_url.as_deref() {
        loader
            .load_worker_async_with_referrer_source(
                document_url,
                entry_referrer_url,
                href,
                max_source_bytes,
                referrer_policy,
            )
            .await
    } else {
        if module_type.is_some() || credentials_mode.is_some() {
            return Err(NativeEngineError::worker_failure(
                "decode parent worker script request",
                NativeWorkerFailureKind::Protocol,
                "Worker entry metadata is inconsistent",
            ));
        }
        loader
            .load_worker_async_with_referrer_policy(
                document_url,
                href,
                max_source_bytes,
                referrer_policy,
            )
            .await
    }
}

pub(in crate::native_engine::content_process) async fn load_parent_page_stylesheet_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeStylesheetResource>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent stylesheet request",
            NativeWorkerFailureKind::Protocol,
            "stylesheet metadata must be an object",
        )
    })?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "integrity" | "crossorigin" | "referrer_policy" | "max_body_bytes"
        )
    }) {
        return Err(NativeEngineError::worker_failure(
            "decode parent stylesheet request",
            NativeWorkerFailureKind::Protocol,
            "stylesheet metadata contains an unknown field",
        ));
    }
    let optional_text = |name: &str| -> Result<Option<String>, NativeEngineError> {
        match object.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_str().map(str::to_owned).map(Some).ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent stylesheet request",
                    NativeWorkerFailureKind::Protocol,
                    format!("stylesheet metadata has an invalid {name}"),
                )
            }),
        }
    };
    let integrity = optional_text("integrity")?;
    let crossorigin = optional_text("crossorigin")?;
    let referrer_policy = object
        .get("referrer_policy")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "decode parent stylesheet request",
                        NativeWorkerFailureKind::Protocol,
                        "stylesheet metadata has an invalid Referrer-Policy",
                    )
                })
                .and_then(NativeFetchReferrerPolicy::parse)
        })
        .transpose()?;
    let max_body_bytes = object
        .get("max_body_bytes")
        .and_then(Value::as_u64)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes > 0 && *bytes <= MAX_CONTENT_STYLESHEET_BYTES)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent stylesheet request",
                NativeWorkerFailureKind::Protocol,
                "stylesheet response limit is invalid",
            )
        })?;
    validate_url_text("parent stylesheet document URL", document_url)?;
    validate_url_text("parent stylesheet target URL", href)?;
    let document = Url::parse(without_fragment(document_url)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent stylesheet request",
            NativeWorkerFailureKind::Protocol,
            "stylesheet document URL is invalid",
        )
    })?;
    let target = Url::parse(href)
        .or_else(|_| document.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent stylesheet request",
                NativeWorkerFailureKind::Protocol,
                "stylesheet target URL is invalid",
            )
        })?;
    if !is_network_url(target.as_str()) {
        return Err(NativeEngineError::worker_failure(
            "decode parent stylesheet request",
            NativeWorkerFailureKind::Protocol,
            "parent stylesheet broker accepts only HTTP(S) targets",
        ));
    }
    let stylesheet = loader
        .load_stylesheet_async_with_object_url_and_referrer_policy(
            document_url,
            href,
            integrity.as_deref(),
            crossorigin.as_deref(),
            None,
            referrer_policy,
        )
        .await?;
    if let Some(stylesheet) = stylesheet.as_ref()
        && stylesheet.body.len() > max_body_bytes
    {
        return Err(NativeEngineError::limit(
            "parent stylesheet response",
            max_body_bytes,
            stylesheet.body.len(),
        ));
    }
    Ok(stylesheet)
}

pub(in crate::native_engine::content_process) async fn load_parent_page_image_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeImage>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent image request",
            NativeWorkerFailureKind::Protocol,
            "image metadata must be an object",
        )
    })?;
    if object.keys().any(|key| key != "referrer_policy") {
        return Err(NativeEngineError::worker_failure(
            "decode parent image request",
            NativeWorkerFailureKind::Protocol,
            "image metadata contains an unknown field",
        ));
    }
    let referrer_policy = object
        .get("referrer_policy")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent image request",
                NativeWorkerFailureKind::Protocol,
                "image metadata omitted its Referrer-Policy",
            )
        })
        .and_then(NativeFetchReferrerPolicy::parse)?;
    validate_url_text("parent image document URL", document_url)?;
    validate_url_text("parent image target URL", href)?;
    let document = Url::parse(without_fragment(document_url)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent image request",
            NativeWorkerFailureKind::Protocol,
            "image document URL is invalid",
        )
    })?;
    let target = Url::parse(href)
        .or_else(|_| document.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent image request",
                NativeWorkerFailureKind::Protocol,
                "image target URL is invalid",
            )
        })?;
    if !is_network_url(target.as_str()) {
        return Err(NativeEngineError::worker_failure(
            "decode parent image request",
            NativeWorkerFailureKind::Protocol,
            "parent image broker accepts only HTTP(S) targets",
        ));
    }
    let image = loader
        .load_image_async_with_object_url_and_referrer_policy(
            document_url,
            href,
            None,
            referrer_policy,
        )
        .await?;
    if image.as_ref().is_some_and(|image| {
        image
            .decoded_bytes()
            .is_none_or(|bytes| bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES)
    }) {
        return Err(NativeEngineError::limit(
            "parent image response",
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
            image
                .as_ref()
                .and_then(NativeImage::decoded_bytes)
                .unwrap_or(usize::MAX),
        ));
    }
    Ok(image)
}

pub(in crate::native_engine::content_process) async fn load_parent_page_video_poster_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeImage>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent video poster request",
            NativeWorkerFailureKind::Protocol,
            "video poster metadata must be an object",
        )
    })?;
    const EXPECTED_FIELDS: [&str; 6] = [
        "client",
        "destination",
        "initiator_type",
        "credentials_mode",
        "use_url_credentials",
        "referrer_policy",
    ];
    if object.len() != EXPECTED_FIELDS.len()
        || object
            .keys()
            .any(|key| !EXPECTED_FIELDS.contains(&key.as_str()))
        || object.get("client").and_then(Value::as_str) != Some("document")
        || object.get("destination").and_then(Value::as_str) != Some("image")
        || object.get("initiator_type").and_then(Value::as_str) != Some("video")
        || object.get("credentials_mode").and_then(Value::as_str) != Some("include")
        || object.get("use_url_credentials").and_then(Value::as_bool) != Some(true)
    {
        return Err(NativeEngineError::worker_failure(
            "decode parent video poster request",
            NativeWorkerFailureKind::Protocol,
            "video poster request metadata does not match the fixed HTML request policy",
        ));
    }
    let referrer_policy = object
        .get("referrer_policy")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent video poster request",
                NativeWorkerFailureKind::Protocol,
                "video poster metadata omitted its Referrer-Policy",
            )
        })
        .and_then(NativeFetchReferrerPolicy::parse)?;
    validate_url_text("parent video poster document URL", document_url)?;
    validate_url_text("parent video poster target URL", href)?;
    let document = Url::parse(without_fragment(document_url)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent video poster request",
            NativeWorkerFailureKind::Protocol,
            "video poster owner Document URL is invalid",
        )
    })?;
    let target = Url::parse(href)
        .or_else(|_| document.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent video poster request",
                NativeWorkerFailureKind::Protocol,
                "video poster target URL is invalid",
            )
        })?;
    if !is_network_url(without_fragment(target.as_str())) {
        return Err(NativeEngineError::worker_failure(
            "decode parent video poster request",
            NativeWorkerFailureKind::Protocol,
            "parent video poster broker accepts only HTTP(S) targets",
        ));
    }
    let image = loader
        .load_video_poster_async_with_referrer_policy(document_url, href, referrer_policy)
        .await?;
    if image.as_ref().is_some_and(|image| {
        image
            .decoded_bytes()
            .is_none_or(|bytes| bytes > MAX_NATIVE_IMAGE_TRANSFER_BYTES)
    }) {
        return Err(NativeEngineError::limit(
            "parent video poster response",
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
            image
                .as_ref()
                .and_then(NativeImage::decoded_bytes)
                .unwrap_or(usize::MAX),
        ));
    }
    Ok(image)
}

pub(in crate::native_engine::content_process) async fn load_parent_page_media_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent media request",
            NativeWorkerFailureKind::Protocol,
            "media metadata must be an object",
        )
    })?;
    if !object.is_empty() {
        return Err(NativeEngineError::worker_failure(
            "decode parent media request",
            NativeWorkerFailureKind::Protocol,
            "media metadata contains an unknown field",
        ));
    }
    validate_url_text("parent media document URL", document_url)?;
    validate_url_text("parent media target URL", href)?;
    let document = Url::parse(without_fragment(document_url)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent media request",
            NativeWorkerFailureKind::Protocol,
            "media document URL is invalid",
        )
    })?;
    let target = Url::parse(href)
        .or_else(|_| document.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent media request",
                NativeWorkerFailureKind::Protocol,
                "media target URL is invalid",
            )
        })?;
    if !is_network_url(target.as_str()) {
        return Err(NativeEngineError::worker_failure(
            "decode parent media request",
            NativeWorkerFailureKind::Protocol,
            "parent media broker accepts only HTTP(S) targets",
        ));
    }
    loader.load_media_async(document_url, href).await
}

pub(in crate::native_engine::content_process) async fn load_parent_page_font_response_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeFetchResponse>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent font request",
            NativeWorkerFailureKind::Protocol,
            "font metadata must be an object",
        )
    })?;
    if !object.is_empty() {
        return Err(NativeEngineError::worker_failure(
            "decode parent font request",
            NativeWorkerFailureKind::Protocol,
            "font metadata contains an unknown field",
        ));
    }
    validate_url_text("parent font document URL", document_url)?;
    validate_url_text("parent font target URL", href)?;
    let document = Url::parse(without_fragment(document_url)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent font request",
            NativeWorkerFailureKind::Protocol,
            "font document URL is invalid",
        )
    })?;
    let target = Url::parse(href)
        .or_else(|_| document.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent font request",
                NativeWorkerFailureKind::Protocol,
                "font target URL is invalid",
            )
        })?;
    if !is_network_url(target.as_str()) {
        return Err(NativeEngineError::worker_failure(
            "decode parent font request",
            NativeWorkerFailureKind::Protocol,
            "parent font broker accepts only HTTP(S) targets",
        ));
    }
    let response = loader
        .load_font_response_async(document_url, href, None)
        .await?;
    if response
        .as_ref()
        .is_some_and(|response| response.body.len() > MAX_NATIVE_FONT_BYTES)
    {
        return Err(NativeEngineError::limit(
            "parent font response",
            MAX_NATIVE_FONT_BYTES,
            response
                .as_ref()
                .map_or(usize::MAX, |response| response.body.len()),
        ));
    }
    Ok(response)
}

pub(in crate::native_engine::content_process) async fn load_parent_page_script_async(
    loader: &mut NativeResourceLoader,
    document_url: &str,
    href: &str,
    value: &Value,
) -> Result<Option<NativeScriptResource>, NativeEngineError> {
    let object = value.as_object().ok_or_else(|| {
        NativeEngineError::worker_failure(
            "decode parent page script request",
            NativeWorkerFailureKind::Protocol,
            "page script metadata must be an object",
        )
    })?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "request_kind"
                | "referrer_url"
                | "max_source_bytes"
                | "parser_inserted"
                | "nonce"
                | "integrity"
                | "crossorigin"
                | "module_type"
                | "referrer_policy"
        )
    }) {
        return Err(NativeEngineError::worker_failure(
            "decode parent page script request",
            NativeWorkerFailureKind::Protocol,
            "page script metadata contains an unknown field",
        ));
    }
    let optional_text = |name: &str| -> Result<Option<String>, NativeEngineError> {
        match object.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_str().map(str::to_owned).map(Some).ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent page script request",
                    NativeWorkerFailureKind::Protocol,
                    format!("page script metadata has an invalid {name}"),
                )
            }),
        }
    };
    let request_kind = object
        .get("request_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent page script request",
                NativeWorkerFailureKind::Protocol,
                "page script metadata omitted its request kind",
            )
        })?;
    let referrer_url = optional_text("referrer_url")?;
    let nonce = optional_text("nonce")?;
    let integrity = optional_text("integrity")?;
    let crossorigin = optional_text("crossorigin")?;
    let module_type = optional_text("module_type")?
        .map(|module_type| match module_type.as_str() {
            "javascript" => Ok(NativeModuleResourceType::JavaScript),
            "json" => Ok(NativeModuleResourceType::Json),
            "unsupported" => Ok(NativeModuleResourceType::Unsupported),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent page script request",
                NativeWorkerFailureKind::Protocol,
                "page script metadata has an unsupported module type",
            )),
        })
        .transpose()?;
    let referrer_policy = optional_text("referrer_policy")?
        .as_deref()
        .map(NativeFetchReferrerPolicy::parse)
        .transpose()?;
    let max_source_bytes = object
        .get("max_source_bytes")
        .and_then(Value::as_u64)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes > 0 && *bytes <= MAX_NATIVE_SCRIPT_BYTES)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent page script request",
                NativeWorkerFailureKind::Protocol,
                "page script source limit is invalid",
            )
        })?;
    let parser_inserted = object
        .get("parser_inserted")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "decode parent page script request",
                NativeWorkerFailureKind::Protocol,
                "page script metadata omitted its parser-inserted flag",
            )
        })?;
    let request_base = referrer_url.as_deref().unwrap_or(document_url);
    validate_url_text("page script request referrer URL", request_base)?;
    let request_base = Url::parse(without_fragment(request_base)).map_err(|_| {
        NativeEngineError::worker_failure(
            "decode parent page script request",
            NativeWorkerFailureKind::Protocol,
            "page script request referrer URL is invalid",
        )
    })?;
    validate_url_text("parent page script target URL", href)?;
    let target = Url::parse(href)
        .or_else(|_| request_base.join(href))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent page script request",
                NativeWorkerFailureKind::Protocol,
                "page script target URL is invalid",
            )
        })?;
    if !is_network_url(target.as_str()) {
        return Err(NativeEngineError::worker_failure(
            "decode parent page script request",
            NativeWorkerFailureKind::Protocol,
            "parent page script broker accepts only HTTP(S) targets",
        ));
    }

    match request_kind {
        "element" if referrer_url.is_none() && module_type.is_none() => {
            loader
                .load_script_async_with_metadata_and_object_url(NativeScriptLoadRequest {
                    document_url,
                    referrer_url: None,
                    href,
                    max_source_bytes,
                    subresource_kind: NativeSubresourceKind::Script,
                    parser_inserted,
                    nonce: nonce.as_deref(),
                    integrity: integrity.as_deref(),
                    crossorigin: crossorigin.as_deref(),
                    object_url: None,
                    module_type: None,
                    worker_credentials_mode: None,
                    referrer_policy,
                })
                .await
        }
        "module_dependency"
            if referrer_url.is_some()
                && parser_inserted
                && nonce.is_none()
                && module_type
                    .is_some_and(|kind| kind != NativeModuleResourceType::Unsupported) =>
        {
            loader
                .load_module_dependency_with_referrer_async(NativeScriptLoadRequest {
                    document_url,
                    referrer_url: referrer_url.as_deref(),
                    href,
                    max_source_bytes,
                    subresource_kind: NativeSubresourceKind::Script,
                    parser_inserted: true,
                    nonce: None,
                    integrity: integrity.as_deref(),
                    crossorigin: Some(crossorigin.as_deref().unwrap_or("anonymous")),
                    object_url: None,
                    module_type,
                    worker_credentials_mode: None,
                    referrer_policy,
                })
                .await
        }
        "dynamic_import"
            if referrer_url.is_some()
                && !parser_inserted
                && nonce.is_none()
                && module_type
                    .is_some_and(|kind| kind != NativeModuleResourceType::Unsupported) =>
        {
            loader
                .load_module_dependency_with_referrer_async(NativeScriptLoadRequest {
                    document_url,
                    referrer_url: referrer_url.as_deref(),
                    href,
                    max_source_bytes,
                    subresource_kind: NativeSubresourceKind::Script,
                    parser_inserted: false,
                    nonce: None,
                    integrity: integrity.as_deref(),
                    crossorigin: Some(crossorigin.as_deref().unwrap_or("anonymous")),
                    object_url: None,
                    module_type,
                    worker_credentials_mode: None,
                    referrer_policy,
                })
                .await
        }
        _ => Err(NativeEngineError::worker_failure(
            "decode parent page script request",
            NativeWorkerFailureKind::Protocol,
            "page script metadata is inconsistent with its request kind",
        )),
    }
}

pub(in crate::native_engine::content_process) fn parent_worker_script_response_payload(
    id: u64,
    fetch_id: u32,
    result: Result<Option<NativeScriptResource>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(resource)) => json!({
            "kind": "worker_script_loaded",
            "id": id,
            "fetch_id": fetch_id,
            "url": resource.url,
            "body": resource.body,
            "response_referrer_policy": resource
                .response_referrer_policy
                .map(NativeFetchReferrerPolicy::as_str),
        }),
        Ok(None) => json!({
            "kind": "worker_script_unavailable",
            "id": id,
            "fetch_id": fetch_id,
        }),
        Err(error) => json!({
            "kind": "error",
            "id": id,
            "fetch_id": fetch_id,
            "reason": error.to_string(),
        }),
    }
}

pub(in crate::native_engine::content_process) fn parent_page_script_response_payload(
    id: u64,
    fetch_id: u32,
    result: Result<Option<NativeScriptResource>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(resource)) => json!({
            "kind": "page_script_loaded",
            "id": id,
            "fetch_id": fetch_id,
            "url": resource.url,
            "body": resource.body,
            "response_referrer_policy": resource
                .response_referrer_policy
                .map(NativeFetchReferrerPolicy::as_str),
        }),
        Ok(None) => json!({
            "kind": "page_script_unavailable",
            "id": id,
            "fetch_id": fetch_id,
        }),
        Err(error) => json!({
            "kind": "error",
            "id": id,
            "fetch_id": fetch_id,
            "reason": error.to_string(),
        }),
    }
}

pub(in crate::native_engine::content_process) fn parent_stylesheet_response_payload(
    id: u64,
    fetch_id: u32,
    result: Result<Option<NativeStylesheetResource>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(stylesheet)) => json!({
            "kind": "page_stylesheet_loaded",
            "id": id,
            "fetch_id": fetch_id,
            "url": stylesheet.url,
            "body": stylesheet.body,
        }),
        Ok(None) => json!({
            "kind": "page_stylesheet_unavailable",
            "id": id,
            "fetch_id": fetch_id,
        }),
        Err(error) => json!({
            "kind": "error",
            "id": id,
            "fetch_id": fetch_id,
            "reason": error.to_string(),
        }),
    }
}

pub(in crate::native_engine::content_process) fn parent_image_response_payload(
    id: u64,
    fetch_id: u32,
    source: &str,
    result: Result<Option<NativeImage>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(image)) => {
            let image_resource = NativeImageResourceWire {
                node_index: 0,
                source: source.to_owned(),
                width: image.width,
                height: image.height,
                pixels_base64: base64::engine::general_purpose::STANDARD.encode(&image.pixels),
                pixels_zlib_base64: String::new(),
                frames: image
                    .frames
                    .iter()
                    .map(|frame| NativeImageFrameWire {
                        delay_ms: frame.delay_ms,
                        pixels_base64: base64::engine::general_purpose::STANDARD
                            .encode(&frame.pixels),
                        pixels_zlib_base64: String::new(),
                    })
                    .collect(),
                loop_count: image.loop_count,
            };
            json!({
                "kind": "page_image_loaded",
                "id": id,
                "fetch_id": fetch_id,
                "image": image_resource,
            })
        }
        Ok(None) => json!({
            "kind": "page_image_unavailable",
            "id": id,
            "fetch_id": fetch_id,
        }),
        Err(error) => json!({
            "kind": "error",
            "id": id,
            "fetch_id": fetch_id,
            "reason": error.to_string(),
        }),
    }
}

pub(in crate::native_engine::content_process) fn parent_media_response_payload(
    id: u64,
    fetch_id: u32,
    result: Result<Option<NativeMediaMetadata>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(metadata)) => json!({
            "kind": "page_media_loaded",
            "id": id,
            "fetch_id": fetch_id,
            "metadata": metadata,
        }),
        Ok(None) => json!({
            "kind": "page_media_unavailable",
            "id": id,
            "fetch_id": fetch_id,
        }),
        Err(error) => json!({
            "kind": "error",
            "id": id,
            "fetch_id": fetch_id,
            "reason": error.to_string(),
        }),
    }
}

pub(in crate::native_engine::content_process) fn parent_font_response_payload(
    id: u64,
    fetch_id: u32,
    result: Result<Option<NativeFetchResponse>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(response)) => {
            let mut payload = parent_fetch_response_payload(id, Ok(response));
            payload["kind"] = Value::String("page_font_loaded".into());
            payload["fetch_id"] = Value::from(fetch_id);
            payload
        }
        Ok(None) => json!({
            "kind": "page_font_unavailable",
            "id": id,
            "fetch_id": fetch_id,
        }),
        Err(error) => json!({
            "kind": "error",
            "id": id,
            "fetch_id": fetch_id,
            "reason": error.to_string(),
        }),
    }
}
