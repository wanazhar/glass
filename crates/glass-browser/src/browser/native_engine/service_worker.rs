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
    MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_POST_MESSAGE_BYTES, MAX_NATIVE_SCRIPT_BYTES,
    MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES, MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
    MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES, MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES,
    MAX_NATIVE_SERVICE_WORKER_CACHES, MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
    MAX_NATIVE_SERVICE_WORKERS, MAX_NATIVE_WORKER_MESSAGES, NativeJavaScriptRuntime,
    NativeMessagePortPageMessage, NativeMessagePortTransfer, NativeScriptCommand,
    NativeScriptEvaluation, NativeServiceWorkerCacheEntry, NativeServiceWorkerCacheState,
    NativeServiceWorkerClientMessage, NativeServiceWorkerRegistrationProfile,
    NativeServiceWorkerRegistrationState, load_service_worker_source,
    validate_message_port_transfers, validate_native_service_worker_cache_request_headers,
};
use super::origin::NativeOrigin;
use super::resource_loader::{
    NativeCorsMode, NativeFetchCacheMode, NativeFetchRedirectMode, NativeFetchRequest,
    NativeFetchResponse, NativeNavigationMethod, NativeNavigationRequest, NativeRequestBody,
    NativeResource, NativeResourceLoader,
};
use base64::Engine as _;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;
use url::Url;

struct NativeServiceWorker {
    id: u32,
    script_url: String,
    scope: String,
    is_module: bool,
    runtime: NativeJavaScriptRuntime,
    import_script_counts: BTreeMap<String, usize>,
}

pub(crate) struct NativeServiceWorkerRegistry {
    registrations: BTreeMap<String, NativeServiceWorker>,
    next_worker_id: u32,
    cache_state: NativeServiceWorkerCacheState,
    registration_profiles: Vec<NativeServiceWorkerRegistrationProfile>,
    pending_message_port_messages: VecDeque<NativeMessagePortPageMessage>,
    pending_client_messages: VecDeque<NativeServiceWorkerClientMessage>,
    message_port_routes: BTreeMap<String, u32>,
}

impl Default for NativeServiceWorkerRegistry {
    fn default() -> Self {
        Self {
            registrations: BTreeMap::new(),
            next_worker_id: 1,
            cache_state: NativeServiceWorkerCacheState::default(),
            registration_profiles: Vec::new(),
            pending_message_port_messages: VecDeque::new(),
            pending_client_messages: VecDeque::new(),
            message_port_routes: BTreeMap::new(),
        }
    }
}

impl NativeServiceWorkerRegistry {
    pub(crate) fn replace_cache_state(&mut self, cache_state: NativeServiceWorkerCacheState) {
        self.cache_state = cache_state;
    }

    pub(crate) fn cache_state(&self) -> &NativeServiceWorkerCacheState {
        &self.cache_state
    }

    pub(crate) fn replace_registration_profiles(
        &mut self,
        profiles: Vec<NativeServiceWorkerRegistrationProfile>,
    ) -> Result<(), NativeEngineError> {
        for profile in &profiles {
            profile.validate()?;
        }
        self.registration_profiles = profiles;
        Ok(())
    }

    pub(crate) fn registration_profiles(&self) -> Vec<NativeServiceWorkerRegistrationProfile> {
        self.registration_profiles.clone()
    }

    pub(crate) async fn restore_for_document(
        &mut self,
        document_url: &str,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        let document = parse_network_url("service worker restoration document URL", document_url)?;
        let document_origin = NativeOrigin::from_url(&document)?;
        let profiles = self.registration_profiles.clone();
        for profile in profiles {
            if self.registrations.contains_key(&profile.scope) {
                continue;
            }
            let scope = Url::parse(&profile.scope).map_err(|_| NativeEngineError::Worker {
                operation: "service worker restoration".into(),
                reason: "persisted service worker scope is invalid".into(),
            })?;
            if NativeOrigin::from_url(&scope)? != document_origin {
                continue;
            }
            let is_module = profile.worker_type.eq_ignore_ascii_case("module");
            let loaded = self.restore_worker(loader, &profile, is_module).await;
            let Ok(worker) = loaded else {
                // A persisted registration must not prevent its page from
                // loading when its script is temporarily unavailable. Keep
                // the profile so the next navigation can retry restoration.
                continue;
            };
            self.registrations.insert(profile.scope.clone(), worker);
        }
        Ok(())
    }

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

    fn next_worker_id(&mut self) -> Result<u32, NativeEngineError> {
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
        Ok(worker_id)
    }

    fn instantiate_worker(
        &mut self,
        script_url: String,
        scope: String,
        is_module: bool,
        source: String,
        import_script_counts: BTreeMap<String, usize>,
        module_sources: BTreeMap<String, String>,
    ) -> Result<NativeServiceWorker, NativeEngineError> {
        let worker_id = self.next_worker_id()?;
        let runtime = NativeJavaScriptRuntime::new_with_context_id(format!(
            "glass-service-worker-{worker_id}"
        ))?;
        if is_module {
            runtime.set_module_sources(module_sources);
        }
        let mut worker = NativeServiceWorker {
            id: worker_id,
            script_url,
            scope,
            is_module,
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
        let client_messages =
            settle_service_worker_cache_event(&mut worker, initial, &mut self.cache_state, None)?;
        self.enqueue_client_messages(client_messages)?;
        Ok(worker)
    }

    async fn restore_worker(
        &mut self,
        loader: &mut NativeResourceLoader,
        profile: &NativeServiceWorkerRegistrationProfile,
        is_module: bool,
    ) -> Result<NativeServiceWorker, NativeEngineError> {
        let resource = loader
            .load_worker_async(
                &profile.script_url,
                &profile.script_url,
                MAX_NATIVE_SCRIPT_BYTES,
            )
            .await?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "service worker restoration".into(),
                reason: "persisted service worker script was blocked or unavailable".into(),
            })?;
        let (source, import_script_counts, module_sources) =
            load_service_worker_source(loader, &profile.script_url, resource.clone(), is_module)
                .await?;
        self.instantiate_worker(
            resource.url,
            profile.scope.clone(),
            is_module,
            source,
            import_script_counts,
            module_sources,
        )
    }

    pub(crate) async fn update(
        &mut self,
        scope: &str,
        loader: &mut NativeResourceLoader,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let profile = self
            .registration_profiles
            .iter()
            .find(|profile| profile.scope == scope)
            .cloned()
            .or_else(|| {
                self.registrations
                    .get(scope)
                    .map(|worker| NativeServiceWorkerRegistrationProfile {
                        script_url: worker.script_url.clone(),
                        scope: worker.scope.clone(),
                        worker_type: if worker.is_module {
                            "module".into()
                        } else {
                            "classic".into()
                        },
                    })
            })
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker update scope",
                    "does not identify a registered worker",
                )
            })?;
        profile.validate()?;
        let is_module = profile.worker_type.eq_ignore_ascii_case("module");
        let resource = loader
            .load_worker_async(
                &profile.script_url,
                &profile.script_url,
                MAX_NATIVE_SCRIPT_BYTES,
            )
            .await?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "service worker update".into(),
                reason: "service worker script was blocked or unavailable".into(),
            })?;
        let (source, import_script_counts, module_sources) =
            load_service_worker_source(loader, &profile.script_url, resource.clone(), is_module)
                .await?;
        let mut worker = self.instantiate_worker(
            resource.url,
            profile.scope.clone(),
            is_module,
            source,
            import_script_counts,
            module_sources,
        )?;
        let install = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "install",
            worker.is_module,
        )?;
        let client_messages =
            settle_service_worker_cache_event(&mut worker, install, &mut self.cache_state, None)?;
        self.enqueue_client_messages(client_messages)?;
        let activate = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "activate",
            worker.is_module,
        )?;
        let client_messages =
            settle_service_worker_cache_event(&mut worker, activate, &mut self.cache_state, None)?;
        self.enqueue_client_messages(client_messages)?;
        if let Some(previous_id) = self.registrations.get(scope).map(|worker| worker.id) {
            self.remove_worker_routes(previous_id);
        }
        let state = NativeServiceWorkerRegistrationState {
            script_url: worker.script_url.clone(),
            scope: worker.scope.clone(),
            state: "activated".into(),
        };
        self.remember_registration(&worker);
        self.registrations.insert(scope.to_owned(), worker);
        Ok(state)
    }

    fn remember_registration(&mut self, worker: &NativeServiceWorker) {
        self.registration_profiles
            .retain(|profile| profile.scope != worker.scope);
        self.registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: worker.script_url.clone(),
                scope: worker.scope.clone(),
                worker_type: if worker.is_module {
                    "module".into()
                } else {
                    "classic".into()
                },
            });
        self.registration_profiles
            .sort_unstable_by(|left, right| left.scope.cmp(&right.scope));
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
            && !self
                .registration_profiles
                .iter()
                .any(|profile| profile.scope == scope)
            && self.registration_profiles.len() >= MAX_NATIVE_SERVICE_WORKERS
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
        let mut worker = self.instantiate_worker(
            resource.url,
            scope.clone(),
            is_module,
            source,
            import_script_counts,
            module_sources,
        )?;
        let install = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "install",
            worker.is_module,
        )?;
        let client_messages =
            settle_service_worker_cache_event(&mut worker, install, &mut self.cache_state, None)?;
        self.enqueue_client_messages(client_messages)?;
        let activate = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "activate",
            worker.is_module,
        )?;
        let client_messages =
            settle_service_worker_cache_event(&mut worker, activate, &mut self.cache_state, None)?;
        self.enqueue_client_messages(client_messages)?;
        let state = NativeServiceWorkerRegistrationState {
            script_url: worker.script_url.clone(),
            scope: scope.clone(),
            state: "activated".into(),
        };
        let previous_id = self.registrations.get(&scope).map(|previous| previous.id);
        if let Some(previous_id) = previous_id {
            self.remove_worker_routes(previous_id);
        }
        self.remember_registration(&worker);
        self.registrations.insert(scope, worker);
        Ok(state)
    }

    pub(crate) fn clear_page_message_port_routes(&mut self) {
        self.pending_message_port_messages.clear();
        self.pending_client_messages.clear();
        self.message_port_routes.clear();
    }

    pub(crate) fn take_message_port_messages(&mut self) -> Vec<NativeMessagePortPageMessage> {
        self.pending_message_port_messages.drain(..).collect()
    }

    pub(crate) fn take_client_messages(&mut self) -> Vec<NativeServiceWorkerClientMessage> {
        self.pending_client_messages.drain(..).collect()
    }

    fn enqueue_client_messages(
        &mut self,
        messages: Vec<NativeServiceWorkerClientMessage>,
    ) -> Result<(), NativeEngineError> {
        if messages.len() > MAX_NATIVE_WORKER_MESSAGES
            || self
                .pending_client_messages
                .len()
                .saturating_add(messages.len())
                > MAX_NATIVE_WORKER_MESSAGES
        {
            return Err(NativeEngineError::limit(
                "native service-worker client messages",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_client_messages
                    .len()
                    .saturating_add(messages.len()),
            ));
        }
        for message in &messages {
            self.register_worker_transfers(message.worker_id, &message.transfer_ports)?;
        }
        self.pending_client_messages.extend(messages);
        Ok(())
    }

    pub(crate) fn unregister(&mut self, scope: &str) -> bool {
        let removed_profile = self
            .registration_profiles
            .iter()
            .any(|profile| profile.scope == scope);
        self.registration_profiles
            .retain(|profile| profile.scope != scope);
        let Some(worker) = self.registrations.remove(scope) else {
            return removed_profile;
        };
        self.remove_worker_routes(worker.id);
        true
    }

    pub(crate) fn post_message(
        &mut self,
        scope: &str,
        data: &Value,
        transfer_ports: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        let Some(worker_id) = self.registrations.get(scope).map(|worker| worker.id) else {
            return Ok(());
        };
        validate_message_port_transfers(transfer_ports)?;
        self.register_page_transfers(worker_id, transfer_ports)?;
        let evaluation = {
            let worker = self
                .registrations
                .get(scope)
                .ok_or_else(|| NativeEngineError::invalid("service worker scope", "not found"))?;
            worker.runtime.dispatch_service_worker_message(
                worker.id,
                &worker.script_url,
                data,
                transfer_ports,
            )
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) => {
                self.remove_transfer_routes(transfer_ports);
                return Err(error);
            }
        };
        let message_port_commands = self
            .registrations
            .get(scope)
            .map(|worker| worker.runtime.take_message_port_commands())
            .unwrap_or_default();
        if let Err(error) = self.collect_message_port_commands(worker_id, message_port_commands) {
            self.remove_transfer_routes(transfer_ports);
            return Err(error);
        }
        let client_messages = match settle_service_worker_cache_event(
            self.registrations
                .get_mut(scope)
                .expect("service worker registration was retained"),
            evaluation,
            &mut self.cache_state,
            None,
        ) {
            Ok(messages) => messages,
            Err(error) => {
                self.remove_transfer_routes(transfer_ports);
                return Err(error);
            }
        };
        if let Err(error) = self.enqueue_client_messages(client_messages) {
            self.remove_transfer_routes(transfer_ports);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn apply_page_message_port_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
    ) -> Result<(), NativeEngineError> {
        if commands.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native page MessagePort commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len(),
            ));
        }
        for command in commands {
            let NativeScriptCommand::MessagePortPostMessage {
                bridge_key,
                data,
                worker_id: None,
                transfer_ports,
            } = command
            else {
                return Err(NativeEngineError::invalid(
                    "native page MessagePort command",
                    "command did not originate from the page realm",
                ));
            };
            let Some(worker_id) = self.message_port_routes.get(&bridge_key).copied() else {
                continue;
            };
            let Some(scope) = self
                .registrations
                .iter()
                .find_map(|(scope, worker)| (worker.id == worker_id).then_some(scope.clone()))
            else {
                self.message_port_routes.remove(&bridge_key);
                continue;
            };
            self.register_page_transfers(worker_id, &transfer_ports)?;
            let evaluation = {
                let worker = self.registrations.get(&scope).ok_or_else(|| {
                    NativeEngineError::invalid("service worker scope", "not found")
                })?;
                worker.runtime.dispatch_service_worker_message_port(
                    worker.id,
                    &worker.script_url,
                    &bridge_key,
                    &data,
                    &transfer_ports,
                )
            };
            let evaluation = match evaluation {
                Ok(evaluation) => evaluation,
                Err(error) => {
                    self.remove_transfer_routes(&transfer_ports);
                    return Err(error);
                }
            };
            let message_port_commands = self
                .registrations
                .get(&scope)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default();
            if let Err(error) = self.collect_message_port_commands(worker_id, message_port_commands)
            {
                self.remove_transfer_routes(&transfer_ports);
                return Err(error);
            }
            let client_messages = match settle_service_worker_cache_event(
                self.registrations
                    .get_mut(&scope)
                    .expect("service worker registration was retained"),
                evaluation,
                &mut self.cache_state,
                None,
            ) {
                Ok(messages) => messages,
                Err(error) => {
                    self.remove_transfer_routes(&transfer_ports);
                    return Err(error);
                }
            };
            if let Err(error) = self.enqueue_client_messages(client_messages) {
                self.remove_transfer_routes(&transfer_ports);
                return Err(error);
            }
        }
        Ok(())
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
        let client_url = without_fragment(document_url);
        let client_id = native_service_worker_client_id(client_url);
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
            "clientId": native_service_worker_client_id(client_url),
            "clientUrl": client_url,
            "clientType": "window",
            "frameType": "top-level",
            "visibilityState": "visible",
            "focused": true,
            "controlled": true,
        });
        let evaluation = worker.runtime.evaluate_service_worker_fetch(
            worker.id,
            &worker.script_url,
            &payload,
            worker.is_module,
        )?;
        let worker_id = worker.id;
        let (value, client_messages) = settle_service_worker_fetch(
            worker,
            loader,
            evaluation,
            &mut self.cache_state,
            &client_id,
        )
        .await?;
        self.enqueue_client_messages(client_messages)?;
        let message_port_commands = self
            .registrations
            .get(&scope)
            .map(|worker| worker.runtime.take_message_port_commands())
            .unwrap_or_default();
        self.collect_message_port_commands(worker_id, message_port_commands)?;
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

    fn register_page_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        self.register_transfers(worker_id, transfers)
    }

    fn register_worker_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        self.register_transfers(worker_id, transfers)
    }

    fn register_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        validate_message_port_transfers(transfers)?;
        if transfers
            .iter()
            .any(|transfer| self.message_port_routes.contains_key(&transfer.bridge_key))
        {
            return Err(NativeEngineError::invalid(
                "native service-worker MessagePort transfer",
                "bridge key was already transferred",
            ));
        }
        for transfer in transfers {
            self.message_port_routes
                .insert(transfer.bridge_key.clone(), worker_id);
        }
        Ok(())
    }

    fn remove_transfer_routes(&mut self, transfers: &[NativeMessagePortTransfer]) {
        for transfer in transfers {
            self.message_port_routes.remove(&transfer.bridge_key);
        }
        let live_bridge_keys = self
            .message_port_routes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.pending_message_port_messages
            .retain(|message| live_bridge_keys.contains(&message.bridge_key));
    }

    fn collect_message_port_commands(
        &mut self,
        worker_id: u32,
        commands: Vec<NativeScriptCommand>,
    ) -> Result<(), NativeEngineError> {
        if commands.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native service-worker MessagePort commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len(),
            ));
        }
        for command in commands {
            let NativeScriptCommand::MessagePortPostMessage {
                bridge_key,
                data,
                worker_id: Some(command_worker_id),
                transfer_ports,
            } = command
            else {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort command",
                    "service worker emitted an invalid MessagePort host command",
                ));
            };
            if command_worker_id != worker_id {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort command",
                    "service worker command owner is invalid",
                ));
            }
            let Some(route_worker_id) = self.message_port_routes.get(&bridge_key).copied() else {
                continue;
            };
            if route_worker_id != worker_id {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort route",
                    "worker id does not own the MessagePort bridge",
                ));
            }
            self.register_worker_transfers(worker_id, &transfer_ports)?;
            if self.pending_message_port_messages.len() >= MAX_NATIVE_WORKER_MESSAGES {
                return Err(NativeEngineError::limit(
                    "native page MessagePort messages",
                    MAX_NATIVE_WORKER_MESSAGES,
                    self.pending_message_port_messages.len().saturating_add(1),
                ));
            }
            self.pending_message_port_messages
                .push_back(NativeMessagePortPageMessage {
                    bridge_key,
                    data,
                    transfer_ports,
                });
        }
        Ok(())
    }

    fn remove_worker_routes(&mut self, worker_id: u32) {
        self.message_port_routes
            .retain(|_, route_worker_id| *route_worker_id != worker_id);
        let live_bridge_keys = self
            .message_port_routes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.pending_message_port_messages
            .retain(|message| live_bridge_keys.contains(&message.bridge_key));
    }
}

fn settle_service_worker_cache_event(
    worker: &mut NativeServiceWorker,
    evaluation: NativeScriptEvaluation,
    cache_state: &mut NativeServiceWorkerCacheState,
    current_client_id: Option<&str>,
) -> Result<Vec<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let mut pending = VecDeque::from(evaluation.commands);
    let mut awaiting = evaluation.top_level_await_pending;
    let mut client_messages = Vec::new();
    let mut turns = 0usize;
    while let Some(command) = pending.pop_front() {
        turns = turns.saturating_add(1);
        if turns > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "service worker cache event turns",
                MAX_NATIVE_MODULE_IMPORTS,
                turns,
            ));
        }
        if let Some(message) =
            service_worker_client_message_command(worker.id, command.clone(), current_client_id)?
        {
            client_messages.push(message);
            continue;
        }
        let (request_id, payload) =
            apply_service_worker_cache_command(worker, command, cache_state)?;
        let resolved = worker.runtime.resolve_service_worker_cache(
            worker.id,
            &worker.script_url,
            request_id,
            &payload,
            worker.is_module,
        )?;
        pending.extend(resolved.commands);
        awaiting |= resolved.top_level_await_pending;
        if worker.runtime.take_top_level_await_result()?.is_some() {
            awaiting = false;
        }
    }
    if awaiting && worker.runtime.take_top_level_await_result()?.is_none() {
        return Err(NativeEngineError::Worker {
            operation: "service worker cache event".into(),
            reason: "service worker cache promise remained pending".into(),
        });
    }
    Ok(client_messages)
}

fn service_worker_client_message_command(
    worker_id: u32,
    command: NativeScriptCommand,
    current_client_id: Option<&str>,
) -> Result<Option<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let NativeScriptCommand::ServiceWorkerClientPostMessage {
        worker_id: command_worker_id,
        client_id,
        data,
        transfer_ports,
    } = command
    else {
        return Ok(None);
    };
    if command_worker_id == 0 || command_worker_id != worker_id {
        return Err(NativeEngineError::Worker {
            operation: "service worker client message".into(),
            reason: "service worker client message owner is invalid".into(),
        });
    }
    if client_id.is_empty() {
        return Err(NativeEngineError::invalid(
            "native service worker client id",
            "must not be empty",
        ));
    }
    if client_id.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker client id",
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            client_id.len(),
        ));
    }
    validate_message_port_transfers(&transfer_ports)?;
    let encoded = serde_json::to_vec(&data).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native service worker client message".into(),
        reason: "service worker client message data could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker client message",
            MAX_NATIVE_POST_MESSAGE_BYTES,
            encoded.len(),
        ));
    }
    if current_client_id != Some(client_id.as_str()) {
        return Ok(None);
    }
    Ok(Some(NativeServiceWorkerClientMessage {
        worker_id,
        client_id,
        data,
        transfer_ports,
    }))
}

fn apply_service_worker_cache_command(
    worker: &mut NativeServiceWorker,
    command: NativeScriptCommand,
    cache_state: &mut NativeServiceWorkerCacheState,
) -> Result<(u32, Value), NativeEngineError> {
    let worker_origin = NativeOrigin::from_url(&parse_network_url(
        "service worker cache owner URL",
        &worker.script_url,
    )?)?
    .serialized();
    let request_id = match &command {
        NativeScriptCommand::ServiceWorkerCacheOpen { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheDelete { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheHas { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheKeys { request_id }
        | NativeScriptCommand::ServiceWorkerCacheMatch { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCachePut { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { request_id, .. }
        | NativeScriptCommand::ServiceWorkerCacheEntries { request_id, .. } => *request_id,
        _ => {
            return Err(NativeEngineError::Worker {
                operation: "service worker cache event".into(),
                reason: "service worker emitted an unsupported host command".into(),
            });
        }
    };
    if request_id == 0 {
        return Err(NativeEngineError::invalid(
            "native service worker cache request id",
            "must be positive",
        ));
    }
    let cache_name = match &command {
        NativeScriptCommand::ServiceWorkerCacheOpen { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheDelete { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheHas { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheMatch { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCachePut { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { cache_name, .. }
        | NativeScriptCommand::ServiceWorkerCacheEntries { cache_name, .. } => cache_name,
        NativeScriptCommand::ServiceWorkerCacheKeys { .. } => "",
        _ => unreachable!(),
    };
    if !cache_name.is_empty() && (cache_name.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES) {
        return Err(NativeEngineError::limit(
            "native service worker cache name",
            MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES,
            cache_name.len(),
        ));
    }
    let previous_state = cache_state.clone();
    let caches = cache_state
        .origins
        .entry(worker_origin.clone())
        .or_default();
    match command {
        NativeScriptCommand::ServiceWorkerCacheOpen { cache_name, .. } => {
            if cache_name.is_empty() {
                return Err(NativeEngineError::invalid(
                    "native service worker cache name",
                    "must not be empty",
                ));
            }
            if !caches.contains_key(&cache_name) && caches.len() >= MAX_NATIVE_SERVICE_WORKER_CACHES
            {
                return Err(NativeEngineError::limit(
                    "native service worker caches",
                    MAX_NATIVE_SERVICE_WORKER_CACHES,
                    caches.len().saturating_add(1),
                ));
            }
            caches.entry(cache_name).or_default();
            cache_state.validate()?;
            Ok((request_id, json!({"ok":true})))
        }
        NativeScriptCommand::ServiceWorkerCacheDelete { cache_name, .. } => {
            let deleted = caches.remove(&cache_name).is_some();
            if caches.is_empty() {
                cache_state.origins.remove(&worker_origin);
            }
            Ok((request_id, json!({"deleted":deleted})))
        }
        NativeScriptCommand::ServiceWorkerCacheHas { cache_name, .. } => {
            Ok((request_id, json!({"has":caches.contains_key(&cache_name)})))
        }
        NativeScriptCommand::ServiceWorkerCacheKeys { .. } => Ok((
            request_id,
            json!({"keys":caches.keys().cloned().collect::<Vec<_>>() }),
        )),
        NativeScriptCommand::ServiceWorkerCacheMatch {
            cache_name,
            request_url,
            request_method,
            request_headers,
            ignore_search,
            ignore_method,
            ignore_vary,
            ..
        } => {
            validate_native_service_worker_cache_request_headers(&request_headers)?;
            let request = validate_cache_request(&request_url, &request_method)?;
            let response = caches
                .get(&cache_name)
                .into_iter()
                .flat_map(|entries| entries.values())
                .find(|entry| {
                    cache_entry_matches(
                        entry,
                        &request,
                        &request_headers,
                        ignore_search,
                        ignore_method,
                        ignore_vary,
                    )
                })
                .map(cache_entry_payload);
            Ok((
                request_id,
                json!({"found":response.is_some(),"response":response}),
            ))
        }
        NativeScriptCommand::ServiceWorkerCachePut {
            cache_name,
            request_url,
            request_method,
            request_headers,
            response,
            ..
        } => {
            let request = validate_cache_request(&request_url, &request_method)?;
            if request.method != "GET" {
                return Err(NativeEngineError::invalid(
                    "native service worker cache request method",
                    "Cache.put only supports GET requests",
                ));
            }
            let entry = cache_entry_from_payload(&response, &request.url, request_headers)?;
            {
                let entries = caches.entry(cache_name).or_default();
                let key = cache_entry_key(&request.method, &request.url);
                if !entries.contains_key(&key)
                    && entries.len() >= MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES
                {
                    return Err(NativeEngineError::limit(
                        "native service worker cache entries",
                        MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
                        entries.len().saturating_add(1),
                    ));
                }
                entries.insert(key, entry);
            }
            if let Err(error) = cache_state.validate() {
                *cache_state = previous_state;
                return Err(error);
            }
            Ok((request_id, json!({"ok":true})))
        }
        NativeScriptCommand::ServiceWorkerCacheDeleteRequest {
            cache_name,
            request_url,
            request_method,
            request_headers,
            ignore_search,
            ignore_method,
            ignore_vary,
            ..
        } => {
            validate_native_service_worker_cache_request_headers(&request_headers)?;
            let request = validate_cache_request(&request_url, &request_method)?;
            let mut deleted = false;
            if let Some(entries) = caches.get_mut(&cache_name) {
                let keys = entries
                    .iter()
                    .filter(|(_, entry)| {
                        cache_entry_matches(
                            entry,
                            &request,
                            &request_headers,
                            ignore_search,
                            ignore_method,
                            ignore_vary,
                        )
                    })
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                for key in keys {
                    deleted |= entries.remove(&key).is_some();
                }
            }
            Ok((request_id, json!({"deleted":deleted})))
        }
        NativeScriptCommand::ServiceWorkerCacheEntries {
            cache_name,
            request_url,
            request_method,
            request_headers,
            ignore_search,
            ignore_method,
            ignore_vary,
            ..
        } => {
            validate_native_service_worker_cache_request_headers(&request_headers)?;
            let request = match (request_url, request_method) {
                (Some(request_url), Some(request_method)) => {
                    Some(validate_cache_request(&request_url, &request_method)?)
                }
                (None, None) => None,
                _ => {
                    return Err(NativeEngineError::invalid(
                        "native service worker cache keys request",
                        "URL and method must be supplied together",
                    ));
                }
            };
            let entries = caches
                .get(&cache_name)
                .into_iter()
                .flat_map(|entries| entries.values())
                .filter(|entry| {
                    request.as_ref().is_none_or(|request| {
                        cache_entry_matches(
                            entry,
                            request,
                            &request_headers,
                            ignore_search,
                            ignore_method,
                            ignore_vary,
                        )
                    })
                })
                .map(|entry| {
                    json!({
                        "url": entry.request_url,
                        "method": entry.method,
                        "headers": entry.request_headers,
                    })
                })
                .collect::<Vec<_>>();
            Ok((request_id, json!({"entries":entries})))
        }
        _ => unreachable!(),
    }
}

struct NativeServiceWorkerCacheRequest {
    url: String,
    method: String,
}

fn validate_cache_request(
    url: &str,
    method: &str,
) -> Result<NativeServiceWorkerCacheRequest, NativeEngineError> {
    let method = method.to_ascii_uppercase();
    if !matches!(
        method.as_str(),
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
    ) {
        return Err(NativeEngineError::invalid(
            "native service worker cache request method",
            "method is unsupported",
        ));
    }
    let url = without_fragment(
        parse_network_url("native service worker cache request URL", url)?.as_str(),
    )
    .to_owned();
    if url.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker cache request URL",
            MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES,
            url.len(),
        ));
    }
    Ok(NativeServiceWorkerCacheRequest { url, method })
}

fn cache_entry_key(method: &str, url: &str) -> String {
    format!("{method}\n{url}")
}

fn cache_match_url(url: &str, ignore_search: bool) -> Result<String, NativeEngineError> {
    let mut parsed = parse_network_url("native service worker cache match URL", url)?;
    if ignore_search {
        parsed.set_query(None);
    }
    Ok(without_fragment(parsed.as_str()).to_owned())
}

fn cache_header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    let values = headers
        .iter()
        .filter(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.join(", "))
}

fn cache_entry_vary_matches(
    entry: &NativeServiceWorkerCacheEntry,
    request_headers: &[(String, String)],
    ignore_vary: bool,
) -> bool {
    if ignore_vary {
        return true;
    }
    for (_, value) in entry
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("vary"))
    {
        for field in value.split(',').map(str::trim) {
            if field == "*" {
                return false;
            }
            if field.is_empty() {
                continue;
            }
            if cache_header_value(&entry.request_headers, field)
                != cache_header_value(request_headers, field)
            {
                return false;
            }
        }
    }
    true
}

fn cache_entry_matches(
    entry: &NativeServiceWorkerCacheEntry,
    request: &NativeServiceWorkerCacheRequest,
    request_headers: &[(String, String)],
    ignore_search: bool,
    ignore_method: bool,
    ignore_vary: bool,
) -> bool {
    if !ignore_method && entry.method != request.method {
        return false;
    }
    let Ok(entry_url) = cache_match_url(&entry.request_url, ignore_search) else {
        return false;
    };
    let Ok(request_url) = cache_match_url(&request.url, ignore_search) else {
        return false;
    };
    entry_url == request_url && cache_entry_vary_matches(entry, request_headers, ignore_vary)
}

fn cache_entry_from_payload(
    value: &Value,
    fallback_url: &str,
    request_headers: Vec<(String, String)>,
) -> Result<NativeServiceWorkerCacheEntry, NativeEngineError> {
    validate_native_service_worker_cache_request_headers(&request_headers)?;
    if value.get("opaque").and_then(Value::as_bool) == Some(true)
        || value.get("opaqueRedirect").and_then(Value::as_bool) == Some(true)
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "opaque service worker responses cannot be persisted in native CacheStorage"
                .into(),
        });
    }
    let response = decode_service_worker_response(value, fallback_url)?;
    if response.body.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker cached response body",
            MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES,
            response.body.len(),
        ));
    }
    let url = response
        .url
        .trim()
        .is_empty()
        .then_some(fallback_url)
        .unwrap_or(&response.url);
    let url = without_fragment(url).to_owned();
    Ok(NativeServiceWorkerCacheEntry {
        method: "GET".into(),
        request_url: without_fragment(fallback_url).to_owned(),
        request_headers,
        url,
        status: response.status,
        status_text: value
            .get("statusText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        headers: response.headers,
        content_type: response.content_type,
        body_base64: base64::engine::general_purpose::STANDARD.encode(response.body),
        body_null: value
            .get("bodyNull")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        redirected: response.redirected,
        opaque: false,
        opaque_redirect: false,
    })
}

fn cache_entry_payload(entry: &NativeServiceWorkerCacheEntry) -> Value {
    json!({
        "url": entry.url,
        "status": entry.status,
        "statusText": entry.status_text,
        "headers": entry.headers,
        "contentType": entry.content_type,
        "bodyBase64": entry.body_base64,
        "bodyNull": entry.body_null,
        "redirected": entry.redirected,
        "opaque": entry.opaque,
        "opaqueRedirect": entry.opaque_redirect,
    })
}

async fn settle_service_worker_fetch(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    evaluation: NativeScriptEvaluation,
    cache_state: &mut NativeServiceWorkerCacheState,
    current_client_id: &str,
) -> Result<(Value, Vec<NativeServiceWorkerClientMessage>), NativeEngineError> {
    let mut pending = VecDeque::from(evaluation.commands);
    let mut value = evaluation.value;
    let mut awaiting = evaluation.top_level_await_pending;
    let mut client_messages = Vec::new();
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
        if let Some(message) = service_worker_client_message_command(
            worker.id,
            command.clone(),
            Some(current_client_id),
        )? {
            client_messages.push(message);
            continue;
        }
        if is_service_worker_cache_command(&command) {
            let (request_id, payload) =
                apply_service_worker_cache_command(worker, command, cache_state)?;
            let resolved = worker.runtime.resolve_service_worker_cache(
                worker.id,
                &worker.script_url,
                request_id,
                &payload,
                worker.is_module,
            )?;
            pending.extend(resolved.commands);
            awaiting |= resolved.top_level_await_pending;
            if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
                value = resolved_value;
                awaiting = false;
            }
            continue;
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
            cache,
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
        let cache_mode = NativeFetchCacheMode::from_option(cache.as_deref())?;
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
                cache_mode,
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
            return Ok((resolved_value, client_messages));
        }
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch response promise remained pending".into(),
        });
    }
    Ok((value, client_messages))
}

fn is_service_worker_cache_command(command: &NativeScriptCommand) -> bool {
    matches!(
        command,
        NativeScriptCommand::ServiceWorkerCacheOpen { .. }
            | NativeScriptCommand::ServiceWorkerCacheDelete { .. }
            | NativeScriptCommand::ServiceWorkerCacheHas { .. }
            | NativeScriptCommand::ServiceWorkerCacheKeys { .. }
            | NativeScriptCommand::ServiceWorkerCacheMatch { .. }
            | NativeScriptCommand::ServiceWorkerCachePut { .. }
            | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { .. }
            | NativeScriptCommand::ServiceWorkerCacheEntries { .. }
    )
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

fn native_service_worker_client_id(document_url: &str) -> String {
    // A content process currently owns one top-level document. Keep the
    // client identity stable for repeated fetches without exposing the full
    // document URL as an opaque Service Worker client id.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in document_url.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("native-client-{hash:016x}")
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
