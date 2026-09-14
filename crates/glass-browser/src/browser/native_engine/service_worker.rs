//! Native service-worker ownership, lifecycle, and request interception.
//!
//! Registrations live with the process-backed browsing context so a reload can
//! be controlled by the worker that the page registered before it. Worker
//! JavaScript remains isolated in its own QuickJS realm; only bounded request,
//! response, and message envelopes cross back to the content-process owner.

use super::config::{validate_url_text, without_fragment};
use super::error::NativeEngineError;
use super::interaction::MAX_NATIVE_FORM_BODY_BYTES;
use super::javascript::{
    MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
    MAX_NATIVE_SERVICE_WORKERS, NativeJavaScriptRuntime, NativeScriptCommand,
    NativeScriptEvaluation, NativeServiceWorkerRegistrationState, load_service_worker_source,
};
use super::origin::NativeOrigin;
use super::resource_loader::{
    NativeCorsMode, NativeFetchRedirectMode, NativeFetchRequest, NativeFetchResponse,
    NativeNavigationMethod, NativeNavigationRequest, NativeRequestBody, NativeResource,
    NativeResourceLoader,
};
use base64::Engine as _;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;
use url::Url;

struct NativeServiceWorker {
    id: u32,
    script_url: String,
    scope: String,
    runtime: NativeJavaScriptRuntime,
    import_script_counts: BTreeMap<String, usize>,
}

pub(crate) struct NativeServiceWorkerRegistry {
    registrations: BTreeMap<String, NativeServiceWorker>,
    next_worker_id: u32,
}

impl Default for NativeServiceWorkerRegistry {
    fn default() -> Self {
        Self {
            registrations: BTreeMap::new(),
            next_worker_id: 1,
        }
    }
}

impl NativeServiceWorkerRegistry {
    pub(crate) fn states_for_document(
        &self,
        document_url: &str,
    ) -> Result<Vec<NativeServiceWorkerRegistrationState>, NativeEngineError> {
        let document = parse_network_url("service worker document URL", document_url)?;
        let document_origin = NativeOrigin::from_url(&document)?;
        Ok(self
            .registrations
            .values()
            .filter_map(|worker| {
                let scope = Url::parse(without_fragment(&worker.scope)).ok()?;
                let scope_origin = NativeOrigin::from_url(&scope).ok()?;
                (scope_origin == document_origin).then_some(NativeServiceWorkerRegistrationState {
                    script_url: worker.script_url.clone(),
                    scope: worker.scope.clone(),
                    state: "activated".into(),
                })
            })
            .collect())
    }

    pub(crate) async fn register(
        &mut self,
        owner_url: &str,
        script_url: &str,
        scope: &str,
        worker_type: &str,
        loader: &mut NativeResourceLoader,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let owner = parse_network_url("service worker owner URL", owner_url)?;
        let script = resolve_same_origin_url(&owner, "service worker script URL", script_url)?;
        let scope = resolve_same_origin_url(&owner, "service worker scope", scope)?;
        let owner_origin = NativeOrigin::from_url(&owner)?;
        if NativeOrigin::from_url(&script)? != owner_origin
            || NativeOrigin::from_url(&scope)? != owner_origin
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "service worker script and scope must be same-origin with the page".into(),
            });
        }
        let script = without_fragment(script.as_str()).to_owned();
        let mut scope = without_fragment(scope.as_str()).to_owned();
        let scope_url = Url::parse(&scope).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "service worker scope URL is invalid".into(),
        })?;
        scope = format!(
            "{}{}",
            scope_url.origin().ascii_serialization(),
            scope_url.path()
        );
        if script.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "service worker script URL",
                MAX_NATIVE_SCRIPT_BYTES,
                script.len(),
            ));
        }
        if scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
            return Err(NativeEngineError::limit(
                "service worker scope",
                MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                scope.len(),
            ));
        }
        let worker_type = if worker_type.is_empty() {
            "classic"
        } else {
            worker_type
        };
        let is_module = worker_type.eq_ignore_ascii_case("module");
        if !is_module && !worker_type.eq_ignore_ascii_case("classic") {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "service worker type must be classic or module".into(),
            });
        }
        let script_path = Url::parse(&script)
            .map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "service worker script URL is invalid".into(),
            })?
            .path()
            .to_owned();
        let script_directory = script_path
            .rfind('/')
            .map(|index| &script_path[..=index])
            .unwrap_or("/");
        if !scope_url.path().starts_with(script_directory) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "service worker scope exceeds the script directory".into(),
            });
        }
        if !self.registrations.contains_key(&scope)
            && self.registrations.len() >= MAX_NATIVE_SERVICE_WORKERS
        {
            return Err(NativeEngineError::limit(
                "native service worker registrations",
                MAX_NATIVE_SERVICE_WORKERS,
                self.registrations.len().saturating_add(1),
            ));
        }
        let resource = loader
            .load_worker_async(owner_url, &script, MAX_NATIVE_SCRIPT_BYTES)
            .await?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "service worker registration".into(),
                reason: "service worker script was blocked or unavailable".into(),
            })?;
        let (source, import_script_counts, module_sources) =
            load_service_worker_source(loader, owner_url, resource.clone(), is_module).await?;
        let worker_id = self.next_worker_id;
        self.next_worker_id = self.next_worker_id.checked_add(1).ok_or_else(|| {
            NativeEngineError::limit(
                "native service worker identifiers",
                u32::MAX as usize,
                u32::MAX as usize,
            )
        })?;
        if worker_id == 0 {
            return Err(NativeEngineError::limit(
                "native service worker identifiers",
                u32::MAX as usize,
                u32::MAX as usize,
            ));
        }
        let runtime = NativeJavaScriptRuntime::new_with_context_id(format!(
            "glass-service-worker-{worker_id}"
        ))?;
        if is_module {
            runtime.set_module_sources(module_sources);
        }
        let worker = NativeServiceWorker {
            id: worker_id,
            script_url: resource.url.clone(),
            scope: scope.clone(),
            runtime,
            import_script_counts,
        };
        let initial = if is_module {
            worker.runtime.evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                Some(&worker.script_url),
                &source,
                &BTreeMap::new(),
            )?
        } else {
            worker.runtime.evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                None,
                &source,
                &worker.import_script_counts,
            )?
        };
        ensure_lifecycle_evaluation("service worker script", initial)?;
        let install = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "install",
        )?;
        ensure_lifecycle_evaluation("service worker install", install)?;
        let activate = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "activate",
        )?;
        ensure_lifecycle_evaluation("service worker activate", activate)?;
        let state = NativeServiceWorkerRegistrationState {
            script_url: worker.script_url.clone(),
            scope: scope.clone(),
            state: "activated".into(),
        };
        self.registrations.insert(scope, worker);
        Ok(state)
    }

    pub(crate) fn unregister(&mut self, scope: &str) -> bool {
        self.registrations.remove(scope).is_some()
    }

    pub(crate) fn post_message(
        &mut self,
        scope: &str,
        data: &Value,
    ) -> Result<(), NativeEngineError> {
        let Some(worker) = self.registrations.get_mut(scope) else {
            return Ok(());
        };
        let evaluation =
            worker
                .runtime
                .dispatch_service_worker_message(worker.id, &worker.script_url, data)?;
        ensure_lifecycle_evaluation("service worker message", evaluation)
    }

    pub(crate) async fn intercept_navigation(
        &mut self,
        loader: &mut NativeResourceLoader,
        navigation: &NativeNavigationRequest,
        _referrer: Option<&str>,
    ) -> Result<Option<NativeResource>, NativeEngineError> {
        let Some(response) = self
            .intercept_fetch(
                loader,
                &navigation.url,
                &navigation.url,
                navigation.method,
                BTreeMap::new(),
                navigation.body.clone(),
                navigation.body_content_type.clone(),
                NativeCorsMode::Navigation,
                NativeFetchRedirectMode::Follow,
                None,
                true,
                "document",
            )
            .await?
        else {
            return Ok(None);
        };
        let url = if response.url.is_empty() {
            without_fragment(&navigation.url).to_owned()
        } else {
            response.url.clone()
        };
        let parsed = parse_network_url("service worker navigation response URL", &url)?;
        Ok(Some(NativeResource {
            url,
            origin: NativeOrigin::from_url(&parsed)?,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn intercept_fetch(
        &mut self,
        loader: &mut NativeResourceLoader,
        document_url: &str,
        href: &str,
        method: NativeNavigationMethod,
        request_headers: BTreeMap<String, String>,
        body: Option<NativeRequestBody>,
        content_type: Option<String>,
        cors_mode: NativeCorsMode,
        redirect_mode: NativeFetchRedirectMode,
        _timeout: Option<Duration>,
        credentials: bool,
        destination: &str,
    ) -> Result<Option<NativeFetchResponse>, NativeEngineError> {
        let owner = parse_network_url("service worker fetch owner URL", document_url)?;
        let target =
            resolve_same_origin_or_cross_origin_url(&owner, "service worker fetch URL", href)?;
        let Some(scope) = self.matching_scope(&target)? else {
            return Ok(None);
        };
        let Some(worker) = self.registrations.get_mut(&scope) else {
            return Ok(None);
        };
        let body_null = body.is_none();
        let (body_text, body_base64) = match body {
            Some(NativeRequestBody::Text(value)) => (Some(value), None),
            Some(NativeRequestBody::Bytes(value)) => (
                None,
                Some(base64::engine::general_purpose::STANDARD.encode(value)),
            ),
            None => (None, None),
        };
        let headers = request_headers
            .into_iter()
            .map(|(name, value)| json!([name, value]))
            .collect::<Vec<_>>();
        let payload = json!({
            "url": without_fragment(target.as_str()),
            "method": method.as_str(),
            "headers": headers,
            "body": body_text,
            "bodyBase64": body_base64,
            "bodyNull": body_null,
            "contentType": content_type,
            "mode": cors_mode_text(cors_mode),
            "redirect": redirect_mode_text(redirect_mode),
            "credentials": credentials,
            "destination": destination,
        });
        let evaluation = worker.runtime.evaluate_service_worker_fetch(
            worker.id,
            &worker.script_url,
            &payload,
        )?;
        let value = settle_service_worker_fetch(worker, loader, evaluation).await?;
        if value.get("handled").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        let response = value
            .get("response")
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker returned no response envelope".into(),
            })?;
        Ok(Some(decode_service_worker_response(
            response,
            target.as_str(),
        )?))
    }

    fn matching_scope(&self, target: &Url) -> Result<Option<String>, NativeEngineError> {
        let target_origin = NativeOrigin::from_url(target)?;
        Ok(self
            .registrations
            .values()
            .filter_map(|worker| {
                let scope = Url::parse(without_fragment(&worker.scope)).ok()?;
                if NativeOrigin::from_url(&scope).ok()? != target_origin
                    || !target.path().starts_with(scope.path())
                {
                    return None;
                }
                Some((scope.path().len(), worker.scope.clone()))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, scope)| scope))
    }
}

fn ensure_lifecycle_evaluation(
    operation: &str,
    evaluation: NativeScriptEvaluation,
) -> Result<(), NativeEngineError> {
    if evaluation.top_level_await_pending || !evaluation.commands.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "service worker emitted an unsupported command during lifecycle dispatch"
                .into(),
        });
    }
    Ok(())
}

async fn settle_service_worker_fetch(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    evaluation: NativeScriptEvaluation,
) -> Result<Value, NativeEngineError> {
    let mut pending = VecDeque::from(evaluation.commands);
    let mut value = evaluation.value;
    let mut awaiting = evaluation.top_level_await_pending;
    let mut resolved_fetches = 0usize;
    while let Some(command) = pending.pop_front() {
        resolved_fetches = resolved_fetches.saturating_add(1);
        if resolved_fetches > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "service worker fetch event turns",
                MAX_NATIVE_MODULE_IMPORTS,
                resolved_fetches,
            ));
        }
        let NativeScriptCommand::Fetch {
            request_id,
            worker_id: Some(worker_id),
            href,
            credentials,
            method,
            headers,
            body,
            body_base64,
            content_type,
            mode,
            redirect,
            timeout_ms,
        } = command
        else {
            return Err(NativeEngineError::Worker {
                operation: "service worker fetch event".into(),
                reason: "service worker emitted an unsupported host command".into(),
            });
        };
        if worker_id != worker.id || request_id == 0 {
            return Err(NativeEngineError::Worker {
                operation: "service worker fetch event".into(),
                reason: "service worker fetch command owner is invalid".into(),
            });
        }
        let request_body = match body_base64 {
            Some(encoded) => Some(NativeRequestBody::Bytes(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "service worker fetch body",
                            "must be valid base64",
                        )
                    })?,
            )),
            None => body.map(NativeRequestBody::Text),
        };
        if request_body
            .as_ref()
            .is_some_and(|body| body.len() > MAX_NATIVE_FORM_BODY_BYTES)
        {
            return Err(NativeEngineError::limit(
                "service worker fetch body",
                MAX_NATIVE_FORM_BODY_BYTES,
                request_body.as_ref().map_or(0, NativeRequestBody::len),
            ));
        }
        let method = NativeNavigationMethod::from_fetch_method(&method)?;
        let cors_mode = parse_cors_mode(mode.as_deref().unwrap_or("same-origin"))?;
        let redirect_mode = parse_redirect_mode(redirect.as_deref().unwrap_or("follow"))?;
        let response = loader
            .fetch_request_with_headers_async(NativeFetchRequest {
                document_url: &worker.script_url,
                href: &href,
                method,
                body: request_body,
                content_type,
                request_headers: headers,
                credentials,
                cors_mode,
                redirect_mode,
                timeout: timeout_ms.map(|value| Duration::from_millis(u64::from(value))),
                max_response_bytes: None,
            })
            .await;
        let payload = match response {
            Ok(response) => service_worker_fetch_payload(response),
            Err(error) => json!({
                "error": error.to_string(),
                "timeout": false,
            }),
        };
        let resolved = worker.runtime.resolve_worker_fetch(
            worker.id,
            &worker.script_url,
            request_id,
            &payload,
            &worker.import_script_counts,
        )?;
        pending.extend(resolved.commands);
        awaiting |= resolved.top_level_await_pending;
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            value = resolved_value;
            awaiting = false;
        }
    }
    if awaiting {
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            return Ok(resolved_value);
        }
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch response promise remained pending".into(),
        });
    }
    Ok(value)
}

fn service_worker_fetch_payload(response: NativeFetchResponse) -> Value {
    json!({
        "url": response.url,
        "status": response.status,
        "contentType": response.content_type,
        "headers": response.headers,
        "body": String::from_utf8_lossy(&response.body),
        "bodyBase64": base64::engine::general_purpose::STANDARD.encode(&response.body),
        "bodyNull": false,
        "redirected": response.redirected,
        "opaque": response.opaque,
        "opaqueRedirect": response.opaque_redirect,
    })
}

fn decode_service_worker_response(
    value: &Value,
    fallback_url: &str,
) -> Result<NativeFetchResponse, NativeEngineError> {
    let status = value
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode service worker response".into(),
            reason: "service worker response status was invalid".into(),
        })?;
    if !(200..=599).contains(&status) {
        return Err(NativeEngineError::Worker {
            operation: "decode service worker response".into(),
            reason: "service worker response status was outside the supported range".into(),
        });
    }
    let body = if value.get("bodyNull").and_then(Value::as_bool) == Some(true) {
        Vec::new()
    } else if let Some(encoded) = value.get("bodyBase64").and_then(Value::as_str) {
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker response body was not valid base64".into(),
            })?
    } else {
        value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .as_bytes()
            .to_vec()
    };
    if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
        return Err(NativeEngineError::limit(
            "service worker response body",
            MAX_NATIVE_FORM_BODY_BYTES,
            body.len(),
        ));
    }
    let headers = value
        .get("headers")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode service worker response".into(),
            reason: "service worker response headers were invalid".into(),
        })?
        .iter()
        .map(|entry| {
            let values = entry.as_array().ok_or_else(|| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker response header entry was invalid".into(),
            })?;
            if values.len() != 2 {
                return Err(NativeEngineError::Worker {
                    operation: "decode service worker response".into(),
                    reason: "service worker response header entry had the wrong arity".into(),
                });
            }
            Ok((
                values[0].as_str().unwrap_or_default().to_owned(),
                values[1].as_str().unwrap_or_default().to_owned(),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if headers.len() > 64 {
        return Err(NativeEngineError::limit(
            "service worker response headers",
            64,
            headers.len(),
        ));
    }
    Ok(NativeFetchResponse {
        url: value
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .unwrap_or(fallback_url)
            .to_owned(),
        status,
        content_type: value
            .get("contentType")
            .and_then(|value| (!value.is_null()).then_some(value))
            .and_then(Value::as_str)
            .map(str::to_owned),
        headers,
        body,
        redirected: value
            .get("redirected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        opaque: false,
        opaque_redirect: false,
    })
}

fn parse_network_url(field: &str, value: &str) -> Result<Url, NativeEngineError> {
    validate_url_text(field, value)?;
    let url =
        Url::parse(without_fragment(value)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must be a valid HTTP(S) URL"),
        })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must use HTTP(S)"),
        });
    }
    if url.username() != "" || url.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must not contain credentials"),
        });
    }
    Ok(url)
}

fn resolve_same_origin_url(
    owner: &Url,
    field: &str,
    value: &str,
) -> Result<Url, NativeEngineError> {
    validate_url_text(field, value)?;
    let url = owner
        .join(value)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{field} is not valid URL syntax"),
        })?;
    let url = parse_network_url(field, url.as_str())?;
    if NativeOrigin::from_url(&url)? != NativeOrigin::from_url(owner)? {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{field} must be same-origin"),
        });
    }
    Ok(url)
}

fn resolve_same_origin_or_cross_origin_url(
    owner: &Url,
    field: &str,
    value: &str,
) -> Result<Url, NativeEngineError> {
    validate_url_text(field, value)?;
    let url = owner
        .join(value)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{field} is not valid URL syntax"),
        })?;
    parse_network_url(field, url.as_str())
}

fn cors_mode_text(mode: NativeCorsMode) -> &'static str {
    match mode {
        NativeCorsMode::NoCors => "no-cors",
        NativeCorsMode::Cors => "cors",
        NativeCorsMode::SameOrigin => "same-origin",
        NativeCorsMode::Navigation => "same-origin",
    }
}

fn parse_cors_mode(value: &str) -> Result<NativeCorsMode, NativeEngineError> {
    match value {
        "no-cors" => Ok(NativeCorsMode::NoCors),
        "cors" => Ok(NativeCorsMode::Cors),
        "same-origin" => Ok(NativeCorsMode::SameOrigin),
        _ => Err(NativeEngineError::invalid(
            "service worker fetch mode",
            "must be no-cors, cors, or same-origin",
        )),
    }
}

fn redirect_mode_text(mode: NativeFetchRedirectMode) -> &'static str {
    match mode {
        NativeFetchRedirectMode::Follow => "follow",
        NativeFetchRedirectMode::Error => "error",
        NativeFetchRedirectMode::Manual => "manual",
    }
}

fn parse_redirect_mode(value: &str) -> Result<NativeFetchRedirectMode, NativeEngineError> {
    match value {
        "follow" => Ok(NativeFetchRedirectMode::Follow),
        "error" => Ok(NativeFetchRedirectMode::Error),
        "manual" => Ok(NativeFetchRedirectMode::Manual),
        _ => Err(NativeEngineError::invalid(
            "service worker fetch redirect mode",
            "must be follow, error, or manual",
        )),
    }
}
