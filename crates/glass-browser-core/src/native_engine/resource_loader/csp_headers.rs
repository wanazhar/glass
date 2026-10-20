use super::*;

pub(super) fn content_security_policy(headers: &HeaderMap) -> NativeCspPolicy {
    let mut policy = NativeCspPolicy::default();
    for value in headers
        .get_all(reqwest::header::CONTENT_SECURITY_POLICY)
        .iter()
    {
        let Ok(value) = value.to_str() else {
            // Preserve the historical fail-closed treatment of an invalid
            // header value without widening it into a new transport error.
            policy.policies.push(NativeCspDirectives {
                style_sources: Some(Vec::new()),
                ..NativeCspDirectives::default()
            });
            continue;
        };
        policy.policies.push(parse_csp_directives(value));
    }
    for value in headers
        .get_all("content-security-policy-report-only")
        .iter()
    {
        let Ok(value) = value.to_str() else {
            continue;
        };
        if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            continue;
        }
        policy.report_only_policies.push(NativeCspDeclaration {
            directives: parse_csp_directives(value),
            original_policy: value.to_owned(),
        });
    }
    for value in headers.get_all("reporting-endpoints").iter() {
        if let Ok(value) = value.to_str() {
            add_reporting_endpoint_header(&mut policy, "reporting-endpoints", value);
        }
    }
    for value in headers.get_all("report-to").iter() {
        if let Ok(value) = value.to_str() {
            add_reporting_endpoint_header(&mut policy, "report-to", value);
        }
    }
    policy.header_policy_count = policy.policies.len();
    policy
}

pub(super) fn document_response_policy_headers(
    headers: &HeaderMap,
) -> Result<Vec<(String, String)>, NativeEngineError> {
    const POLICY_HEADERS: [&str; 5] = [
        "content-security-policy",
        "content-security-policy-report-only",
        "reporting-endpoints",
        "report-to",
        "referrer-policy",
    ];
    let mut pairs = Vec::new();
    let mut total_bytes = 0usize;
    for name in POLICY_HEADERS {
        for value in headers.get_all(name).iter() {
            let value = value.to_str().map_err(|_| NativeEngineError::Network {
                operation: "document response policy".into(),
                reason: "response contains a non-text policy header".into(),
            })?;
            if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
                return Err(NativeEngineError::limit(
                    "document response policy header value",
                    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                    value.len(),
                ));
            }
            total_bytes = total_bytes
                .saturating_add(name.len())
                .saturating_add(value.len());
            if pairs.len() >= MAX_NATIVE_RESPONSE_HEADERS {
                return Err(NativeEngineError::limit(
                    "document response policy headers",
                    MAX_NATIVE_RESPONSE_HEADERS,
                    pairs.len().saturating_add(1),
                ));
            }
            if total_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
                return Err(NativeEngineError::limit(
                    "document response policy header bytes",
                    MAX_NATIVE_RESPONSE_HEADER_BYTES,
                    total_bytes,
                ));
            }
            pairs.push((name.to_owned(), value.to_owned()));
        }
    }
    Ok(pairs)
}

pub(super) fn merge_document_response_policy_headers(
    current: &mut Vec<(String, String)>,
    updates: Vec<(String, String)>,
) {
    let updated_names = updates
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    current.retain(|(name, _)| !updated_names.contains(&name.to_ascii_lowercase()));
    current.extend(updates);
}

pub(super) fn parse_csp_directives(value: &str) -> NativeCspDirectives {
    let mut policy = NativeCspDirectives::default();
    for directive in value.split(';') {
        let mut parts = directive.split_ascii_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let sources = parts.map(str::to_owned).collect::<Vec<_>>();
        match name.to_ascii_lowercase().as_str() {
            "style-src" => policy.style_sources = Some(sources),
            "style-src-elem" => policy.style_element_sources = Some(sources),
            "style-src-attr" => policy.style_attribute_sources = Some(sources),
            "script-src" => policy.script_sources = Some(sources),
            "script-src-elem" => policy.script_element_sources = Some(sources),
            "script-src-attr" => policy.script_attribute_sources = Some(sources),
            "img-src" => policy.image_sources = Some(sources),
            "font-src" => policy.font_sources = Some(sources),
            "media-src" => policy.media_sources = Some(sources),
            "frame-src" => policy.frame_sources = Some(sources),
            "child-src" => policy.child_sources = Some(sources),
            "connect-src" => policy.connect_sources = Some(sources),
            "worker-src" => policy.worker_sources = Some(sources),
            "form-action" => policy.form_action_sources = Some(sources),
            "navigate-to" => policy.navigate_to_sources = Some(sources),
            "default-src" => policy.default_sources = Some(sources),
            "report-uri" => {
                policy.report_uris = sources
                    .into_iter()
                    .take(MAX_NATIVE_CSP_REPORT_ENDPOINTS)
                    .collect()
            }
            "report-to" => policy.report_to = sources.into_iter().next(),
            _ => {}
        }
    }
    policy
}

pub(super) fn add_reporting_endpoint_header(policy: &mut NativeCspPolicy, name: &str, value: &str) {
    if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
        return;
    }
    let entries = if name.eq_ignore_ascii_case("reporting-endpoints") {
        parse_reporting_endpoints_header(value)
    } else {
        parse_legacy_report_to_header(value)
    };
    for (group, endpoint) in entries {
        if group.is_empty()
            || group.len() > MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES
            || endpoint.len() > MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES
        {
            continue;
        }
        let endpoints = policy.reporting_endpoints.entry(group).or_default();
        if endpoints.len() < MAX_NATIVE_CSP_REPORT_ENDPOINTS
            && !endpoints.iter().any(|current| current == &endpoint)
        {
            endpoints.push(endpoint);
        }
    }
}

pub(super) fn parse_reporting_endpoints_header(value: &str) -> Vec<(String, String)> {
    value
        .split(',')
        .filter_map(|member| {
            let (group, endpoint) = member.split_once('=')?;
            let group = group.trim();
            let endpoint = endpoint.trim();
            let endpoint = endpoint
                .strip_prefix('"')
                .and_then(|endpoint| endpoint.strip_suffix('"'))
                .unwrap_or(endpoint);
            (!group.is_empty() && !endpoint.is_empty())
                .then(|| (group.to_owned(), endpoint.to_owned()))
        })
        .take(MAX_NATIVE_CSP_REPORT_ENDPOINTS)
        .collect()
}

pub(super) fn parse_legacy_report_to_header(value: &str) -> Vec<(String, String)> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(value) else {
        return Vec::new();
    };
    let objects: Vec<&serde_json::Value> = match &parsed {
        serde_json::Value::Array(values) => values.iter().collect(),
        serde_json::Value::Object(_) => vec![&parsed],
        _ => Vec::new(),
    };
    let mut entries = Vec::new();
    for object in objects {
        if object
            .get("max_age")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
            == 0
        {
            continue;
        }
        let Some(group) = object.get("group").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(endpoints) = object
            .get("endpoints")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for endpoint in endpoints.iter().take(MAX_NATIVE_CSP_REPORT_ENDPOINTS) {
            let Some(url) = endpoint.get("url").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !url.is_empty() {
                entries.push((group.to_owned(), url.to_owned()));
            }
        }
        if entries.len() >= MAX_NATIVE_CSP_REPORT_ENDPOINTS {
            break;
        }
    }
    entries
}

pub(crate) fn resolve_subresource_url(
    document_url: &Url,
    href: &str,
) -> Result<Option<Url>, NativeEngineError> {
    let target_url = document_url
        .join(href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "subresource URL could not be resolved against the document".into(),
        })?;
    reject_credentials(&target_url)?;
    if !is_network_url(without_fragment(target_url.as_str())) {
        return Ok(None);
    }
    Ok(Some(target_url))
}
