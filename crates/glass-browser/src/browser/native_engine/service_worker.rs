//! Native service-worker ownership, lifecycle, and request interception.
//!
//! Registrations live with the process-backed browsing context so a reload can
//! be controlled by the worker that the page registered before it. Worker
//! JavaScript remains isolated in its own QuickJS realm; only bounded request,
//! response, and message envelopes cross back to the content-process owner.

use super::config::{is_network_url, validate_context_id, validate_url_text, without_fragment};
use super::error::NativeEngineError;
use super::fetch_stream::{
    MAX_NATIVE_FETCH_UPLOAD_CHUNKS, NativeFetchUploadCommand, NativeFetchUploadConnection,
    NativeFetchUploadEvent, spawn_native_fetch_upload_stream,
};
use super::interaction::{MAX_NATIVE_EFFECTS, MAX_NATIVE_FORM_BODY_BYTES};
use super::javascript::{
    MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES,
    MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES, MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES,
    MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES, MAX_NATIVE_SERVICE_WORKER_CACHES,
    MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES, MAX_NATIVE_SERVICE_WORKERS, MAX_NATIVE_WORKER_MESSAGES,
    NativeJavaScriptRuntime, NativeMessagePortPageMessage, NativeMessagePortTransfer,
    NativeScriptCommand, NativeScriptEvaluation, NativeServiceWorkerCacheBatchEntry,
    NativeServiceWorkerCacheEntry, NativeServiceWorkerCacheState, NativeServiceWorkerClientMessage,
    NativeServiceWorkerClientState, NativeServiceWorkerOpenWindowRequest,
    NativeServiceWorkerRegistrationProfile, NativeServiceWorkerRegistrationState,
    NativeServiceWorkerWorkerProfile, NativeServiceWorkerWorkerState, NativeWorkerModuleGraph,
    default_navigation_preload_header_value, load_service_worker_source,
    resolve_module_request_url, validate_message_port_transfers, validate_native_message_payload,
    validate_native_object_url_transfers, validate_native_service_worker_cache_request_headers,
    validate_navigation_preload_header_value, validate_service_worker_client_states,
};
use super::origin::NativeOrigin;
use super::resource_loader::{
    NativeCorsMode, NativeFetchCacheMode, NativeFetchCredentialsMode, NativeFetchMethod,
    NativeFetchRedirectMode, NativeFetchReferrerPolicy, NativeFetchRequest, NativeFetchResponse,
    NativeNavigationRequest, NativeRequestBody, NativeResource, NativeResourceLoader,
    referrer_for_navigation_with_policy,
};
use base64::Engine as _;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;
use url::Url;

const MAX_NATIVE_SERVICE_WORKER_CLIENTS: usize =
    crate::browser::session::TOPOLOGY_MAX_TARGETS * crate::browser::session::TOPOLOGY_MAX_FRAMES;

struct NativeServiceWorker {
    id: u32,
    script_url: String,
    scope: String,
    is_module: bool,
    runtime: NativeJavaScriptRuntime,
    import_script_counts: BTreeMap<String, usize>,
    skip_waiting_requested: bool,
    clients_claim_requested: bool,
    fetch_upload_connections: BTreeMap<u32, NativeFetchUploadConnection>,
}

const MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS: usize = MAX_NATIVE_WORKER_MESSAGES;

pub(crate) enum NativeServiceWorkerNavigationOutcome {
    NotHandled,
    Handled(NativeResource),
    Suspended,
}

pub(crate) enum NativeServiceWorkerFetchOutcome {
    NotHandled,
    Handled(NativeFetchResponse),
    Suspended,
}

pub(crate) struct NativeServiceWorkerFetchCompletion {
    pub(crate) worker_id: u32,
    pub(crate) response: Option<NativeFetchResponse>,
}

pub(crate) enum NativeServiceWorkerNavigationPreloadResult {
    State { enabled: bool, header_value: String },
    InvalidState,
}

pub(super) struct NativeServiceWorkerNavigationPreloadRequest {
    request_id: u32,
    scope: String,
    header_value: String,
}

struct NativeServiceWorkerFetchContinuation {
    worker_id: u32,
    open_window_request_id: u32,
    fallback_url: String,
    pending: VecDeque<NativeScriptCommand>,
    value: Value,
    awaiting: bool,
}

enum NativeServiceWorkerFetchSettlement {
    Complete {
        value: Value,
        client_messages: Vec<NativeServiceWorkerClientMessage>,
    },
    Suspended {
        continuation: NativeServiceWorkerFetchContinuation,
        client_messages: Vec<NativeServiceWorkerClientMessage>,
    },
}

fn completed_lifecycle_states() -> Vec<String> {
    ["installing", "installed", "activating", "activated"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn waiting_lifecycle_states() -> Vec<String> {
    ["installing", "installed"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn worker_state(worker: &NativeServiceWorker, state: &str) -> NativeServiceWorkerWorkerState {
    NativeServiceWorkerWorkerState {
        script_url: worker.script_url.clone(),
        state: state.to_owned(),
    }
}

fn worker_type_name(worker: &NativeServiceWorker) -> String {
    if worker.is_module {
        "module".into()
    } else {
        "classic".into()
    }
}

fn registration_state_for_workers(
    scope: &str,
    active: Option<&NativeServiceWorker>,
    waiting: Option<&NativeServiceWorker>,
    controlled: bool,
    lifecycle: Vec<String>,
) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
    let active_state = active.map(|worker| worker_state(worker, "activated"));
    let waiting_state = waiting.map(|worker| worker_state(worker, "installed"));
    let representative = active_state
        .as_ref()
        .or(waiting_state.as_ref())
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "native service worker registration state",
                "must contain an active or waiting worker",
            )
        })?;
    Ok(NativeServiceWorkerRegistrationState {
        script_url: representative.script_url.clone(),
        scope: scope.to_owned(),
        state: representative.state.clone(),
        controlled,
        lifecycle,
        active: active_state,
        waiting: waiting_state,
    })
}

pub(crate) struct NativeServiceWorkerRegistry {
    registrations: BTreeMap<String, NativeServiceWorker>,
    waiting_workers: BTreeMap<String, NativeServiceWorker>,
    next_worker_id: u32,
    next_navigation_preload_request_id: u32,
    next_timer_worker_id: u32,
    cache_state: NativeServiceWorkerCacheState,
    registration_profiles: Vec<NativeServiceWorkerRegistrationProfile>,
    registration_changes: BTreeMap<String, Option<NativeServiceWorkerRegistrationProfile>>,
    pending_message_port_messages: VecDeque<NativeMessagePortPageMessage>,
    pending_client_messages: VecDeque<NativeServiceWorkerClientMessage>,
    pending_open_windows: VecDeque<NativeServiceWorkerOpenWindowRequest>,
    pending_fetches: BTreeMap<(u32, u32), NativeServiceWorkerFetchContinuation>,
    completed_fetches: VecDeque<NativeServiceWorkerFetchCompletion>,
    announced_open_windows: BTreeSet<(u32, u32)>,
    message_port_routes: BTreeMap<String, u32>,
    current_client_id: String,
    current_client_url: Option<String>,
    current_client_scope: Option<String>,
    client_states: Vec<NativeServiceWorkerClientState>,
}

impl Default for NativeServiceWorkerRegistry {
    fn default() -> Self {
        Self {
            registrations: BTreeMap::new(),
            waiting_workers: BTreeMap::new(),
            next_worker_id: 1,
            next_navigation_preload_request_id: 1,
            next_timer_worker_id: 0,
            cache_state: NativeServiceWorkerCacheState::default(),
            registration_profiles: Vec::new(),
            registration_changes: BTreeMap::new(),
            pending_message_port_messages: VecDeque::new(),
            pending_client_messages: VecDeque::new(),
            pending_open_windows: VecDeque::new(),
            pending_fetches: BTreeMap::new(),
            completed_fetches: VecDeque::new(),
            announced_open_windows: BTreeSet::new(),
            message_port_routes: BTreeMap::new(),
            current_client_id: String::new(),
            current_client_url: None,
            current_client_scope: None,
            client_states: Vec::new(),
        }
    }
}

impl NativeServiceWorkerRegistry {
    pub(crate) fn begin_document(
        &mut self,
        document_url: &str,
        client_id: Option<&str>,
    ) -> Result<(), NativeEngineError> {
        let document = parse_network_url("service worker document URL", document_url)?;
        let document_url = without_fragment(document.as_str()).to_owned();
        if let Some(client_id) = client_id {
            validate_context_id(client_id)?;
            if client_id.is_empty() {
                return Err(NativeEngineError::invalid(
                    "native service worker client id",
                    "must not be empty",
                ));
            }
        }
        self.current_client_id = client_id
            .map(str::to_owned)
            .unwrap_or_else(|| native_service_worker_url_client_id(&document_url));
        self.current_client_url = Some(document_url);
        self.current_client_scope = None;
        self.ensure_current_client_state()?;
        Ok(())
    }

    pub(crate) fn commit_document(&mut self, document_url: &str) -> Result<(), NativeEngineError> {
        let document = parse_network_url("service worker document URL", document_url)?;
        self.current_client_url = Some(without_fragment(document.as_str()).to_owned());
        self.current_client_scope = self.matching_scope(&document)?;
        self.update_current_client_state_url(document.as_str());
        Ok(())
    }

    fn current_client_id_for(&self, document_url: &str) -> String {
        let canonical = without_fragment(document_url);
        if self.current_client_url.as_deref() == Some(canonical)
            && !self.current_client_id.is_empty()
        {
            return self.current_client_id.clone();
        }
        native_service_worker_url_client_id(canonical)
    }

    fn current_client_is_controlled(&self, document_url: &str) -> bool {
        without_fragment(document_url) == self.current_client_url.as_deref().unwrap_or_default()
            && self.current_client_scope.is_some()
    }

    fn current_client_scope_for(&self, document_url: &str) -> Option<&str> {
        self.current_client_is_controlled(document_url)
            .then_some(self.current_client_scope.as_deref())
            .flatten()
    }

    fn claim_current_client(&mut self, scope: &str) -> Result<(), NativeEngineError> {
        let Some(document_url) = self.current_client_url.as_deref() else {
            return Ok(());
        };
        let document = parse_network_url("service worker client URL", document_url)?;
        if self.matching_scope(&document)?.as_deref() == Some(scope) {
            self.current_client_scope = Some(scope.to_owned());
        }
        Ok(())
    }

    pub(crate) fn replace_client_states(
        &mut self,
        states: Vec<NativeServiceWorkerClientState>,
    ) -> Result<(), NativeEngineError> {
        validate_service_worker_client_states(&states)?;
        self.client_states = states;
        self.ensure_current_client_state()
    }

    fn ensure_current_client_state(&mut self) -> Result<(), NativeEngineError> {
        let Some(client_url) = self.current_client_url.clone() else {
            return Ok(());
        };
        if let Some(state) = self
            .client_states
            .iter_mut()
            .find(|state| state.id == self.current_client_id)
        {
            state.url = client_url;
            return Ok(());
        }
        if self.client_states.len() >= MAX_NATIVE_SERVICE_WORKER_CLIENTS {
            return Err(NativeEngineError::limit(
                "native service worker clients",
                MAX_NATIVE_SERVICE_WORKER_CLIENTS,
                self.client_states.len().saturating_add(1),
            ));
        }
        self.client_states.push(NativeServiceWorkerClientState {
            id: self.current_client_id.clone(),
            url: client_url,
            client_type: "window".into(),
            frame_type: "top-level".into(),
            visibility_state: "visible".into(),
            focused: true,
        });
        Ok(())
    }

    fn update_current_client_state_url(&mut self, document_url: &str) {
        let canonical = without_fragment(document_url);
        if let Some(state) = self
            .client_states
            .iter_mut()
            .find(|state| state.id == self.current_client_id)
        {
            state.url = canonical.to_owned();
        }
    }

    fn client_states_for_worker(&self, scope: &str) -> Result<Vec<Value>, NativeEngineError> {
        let scope_url =
            Url::parse(without_fragment(scope)).map_err(|_| NativeEngineError::Worker {
                operation: "service worker clients".into(),
                reason: "service worker scope is invalid".into(),
            })?;
        let scope_origin = NativeOrigin::from_url(&scope_url)?;
        let mut clients = Vec::new();
        for state in &self.client_states {
            let Ok(client_url) = Url::parse(without_fragment(&state.url)) else {
                continue;
            };
            if !is_network_url(client_url.as_str())
                || NativeOrigin::from_url(&client_url)? != scope_origin
            {
                continue;
            }
            let client_url_text = without_fragment(client_url.as_str());
            let controlled = if state.id == self.current_client_id
                && self.current_client_url.as_deref() == Some(client_url_text)
            {
                self.current_client_scope.as_deref() == Some(scope)
            } else {
                self.matching_scope(&client_url)?.as_deref() == Some(scope)
            };
            clients.push(json!({
                "clientId": state.id,
                "clientUrl": client_url_text,
                "clientType": state.client_type,
                "frameType": state.frame_type,
                "visibilityState": state.visibility_state,
                "focused": state.focused,
                "controlled": controlled,
            }));
        }
        Ok(clients)
    }

    fn set_worker_client_view(
        &self,
        worker: &NativeServiceWorker,
    ) -> Result<Vec<Value>, NativeEngineError> {
        let clients = self.client_states_for_worker(&worker.scope)?;
        worker.runtime.set_service_worker_clients(clients.clone());
        Ok(clients)
    }

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
        let mut removed_worker_ids = Vec::new();
        self.registrations.retain(|scope, worker| {
            let keep = profiles.iter().any(|profile| {
                profile.scope == *scope
                    && profile.script_url == worker.script_url
                    && profile
                        .worker_type
                        .eq_ignore_ascii_case(&worker_type_name(worker))
            });
            if !keep {
                removed_worker_ids.push(worker.id);
            }
            keep
        });
        self.waiting_workers.retain(|scope, worker| {
            let keep = profiles.iter().any(|profile| {
                profile.scope == *scope
                    && profile.waiting.as_ref().is_some_and(|waiting| {
                        waiting.script_url == worker.script_url
                            && waiting
                                .worker_type
                                .eq_ignore_ascii_case(&worker_type_name(worker))
                    })
            });
            if !keep {
                removed_worker_ids.push(worker.id);
            }
            keep
        });
        for worker_id in removed_worker_ids {
            self.remove_worker_routes(worker_id);
        }
        if self
            .current_client_scope
            .as_ref()
            .is_some_and(|scope| !profiles.iter().any(|profile| profile.scope == *scope))
        {
            self.current_client_scope = None;
        }
        self.registration_profiles = profiles;
        Ok(())
    }

    pub(crate) fn registration_profiles(&self) -> Vec<NativeServiceWorkerRegistrationProfile> {
        self.registration_profiles.clone()
    }

    pub(crate) fn registration_changes(
        &self,
    ) -> &BTreeMap<String, Option<NativeServiceWorkerRegistrationProfile>> {
        &self.registration_changes
    }

    pub(crate) fn apply_navigation_preload_operation(
        &mut self,
        document_url: &str,
        scope: &str,
        operation: &str,
        header_value: Option<&str>,
    ) -> Result<NativeServiceWorkerNavigationPreloadResult, NativeEngineError> {
        let document = parse_network_url("navigation preload document URL", document_url)?;
        let registration_scope = parse_network_url("navigation preload registration scope", scope)?;
        if NativeOrigin::from_url(&document)? != NativeOrigin::from_url(&registration_scope)? {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "navigation preload scope must be same-origin with its document".into(),
            });
        }

        let profile_index = self
            .registration_profiles
            .iter()
            .position(|profile| profile.scope == scope);
        let registration_exists = profile_index.is_some()
            || self.registrations.contains_key(scope)
            || self.waiting_workers.contains_key(scope);
        if !registration_exists {
            return Err(NativeEngineError::invalid(
                "native service worker navigation preload scope",
                "does not identify a registration",
            ));
        }
        if !matches!(
            operation,
            "enable" | "disable" | "setHeaderValue" | "getState"
        ) {
            return Err(NativeEngineError::invalid(
                "native service worker navigation preload operation",
                "is not supported",
            ));
        }
        if operation == "setHeaderValue" {
            validate_navigation_preload_header_value(header_value.ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker navigation preload header value",
                    "is required for setHeaderValue",
                )
            })?)?;
        } else if header_value.is_some() {
            return Err(NativeEngineError::invalid(
                "native service worker navigation preload header value",
                "is only valid for setHeaderValue",
            ));
        }

        if operation != "getState" && !self.registrations.contains_key(scope) {
            return Ok(NativeServiceWorkerNavigationPreloadResult::InvalidState);
        }

        let profile_index = profile_index.ok_or_else(|| {
            NativeEngineError::invalid(
                "native service worker navigation preload profile",
                "registration settings are unavailable",
            )
        })?;
        let profile = &mut self.registration_profiles[profile_index];
        let changed = match operation {
            "enable" if !profile.navigation_preload_enabled => {
                profile.navigation_preload_enabled = true;
                true
            }
            "disable" if profile.navigation_preload_enabled => {
                profile.navigation_preload_enabled = false;
                true
            }
            "setHeaderValue" => {
                let value = header_value.expect("setHeaderValue input was validated");
                if profile.navigation_preload_header_value == value {
                    false
                } else {
                    profile.navigation_preload_header_value = value.to_owned();
                    true
                }
            }
            "enable" | "disable" | "getState" => false,
            _ => unreachable!("NavigationPreload operation was validated above"),
        };
        let state = NativeServiceWorkerNavigationPreloadResult::State {
            enabled: profile.navigation_preload_enabled,
            header_value: profile.navigation_preload_header_value.clone(),
        };
        if changed {
            self.record_registration_change(scope);
        }
        Ok(state)
    }

    pub(crate) fn clear_registration_changes(&mut self) {
        self.registration_changes.clear();
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
            let scope = Url::parse(&profile.scope).map_err(|_| NativeEngineError::Worker {
                operation: "service worker restoration".into(),
                reason: "persisted service worker scope is invalid".into(),
            })?;
            if NativeOrigin::from_url(&scope)? != document_origin {
                continue;
            }
            if !self.registrations.contains_key(&profile.scope) {
                let loaded = self
                    .restore_worker(
                        loader,
                        &profile.script_url,
                        &profile.scope,
                        &profile.worker_type,
                    )
                    .await;
                if let Ok(worker) = loaded {
                    self.registrations.insert(profile.scope.clone(), worker);
                }
                // A persisted registration must not prevent its page from
                // loading when its active script is temporarily unavailable.
                // Keep the profile so a later navigation can retry it.
            }
            let Some(waiting) = profile.waiting.as_ref() else {
                continue;
            };
            if self.waiting_workers.contains_key(&profile.scope) {
                continue;
            }
            let loaded = self
                .restore_worker(
                    loader,
                    &waiting.script_url,
                    &profile.scope,
                    &waiting.worker_type,
                )
                .await;
            if let Ok(worker) = loaded {
                self.waiting_workers.insert(profile.scope.clone(), worker);
            }
        }
        Ok(())
    }

    pub(crate) fn states_for_document(
        &self,
        document_url: &str,
    ) -> Result<Vec<NativeServiceWorkerRegistrationState>, NativeEngineError> {
        let document = parse_network_url("service worker document URL", document_url)?;
        let document_origin = NativeOrigin::from_url(&document)?;
        let controlled_scope = self.current_client_scope_for(document_url);
        let scopes = self
            .registrations
            .keys()
            .chain(self.waiting_workers.keys())
            .chain(
                self.registration_profiles
                    .iter()
                    .map(|profile| &profile.scope),
            )
            .cloned()
            .collect::<BTreeSet<_>>();
        Ok(scopes
            .into_iter()
            .filter_map(|scope_text| {
                let scope = Url::parse(without_fragment(&scope_text)).ok()?;
                let scope_origin = NativeOrigin::from_url(&scope).ok()?;
                if scope_origin != document_origin {
                    return None;
                }
                let profile = self
                    .registration_profiles
                    .iter()
                    .find(|profile| profile.scope == scope_text);
                let active = self
                    .registrations
                    .get(&scope_text)
                    .map(|worker| NativeServiceWorkerWorkerState {
                        script_url: worker.script_url.clone(),
                        state: "activated".into(),
                    })
                    .or_else(|| {
                        profile.map(|profile| NativeServiceWorkerWorkerState {
                            script_url: profile.script_url.clone(),
                            state: "activated".into(),
                        })
                    });
                let waiting = self
                    .waiting_workers
                    .get(&scope_text)
                    .map(|worker| NativeServiceWorkerWorkerState {
                        script_url: worker.script_url.clone(),
                        state: "installed".into(),
                    })
                    .or_else(|| {
                        profile
                            .and_then(|profile| profile.waiting.as_ref())
                            .map(|waiting| NativeServiceWorkerWorkerState {
                                script_url: waiting.script_url.clone(),
                                state: "installed".into(),
                            })
                    });
                let representative = active.as_ref().or(waiting.as_ref())?;
                let controlled = controlled_scope == Some(scope_text.as_str());
                Some(NativeServiceWorkerRegistrationState {
                    script_url: representative.script_url.clone(),
                    scope: scope_text,
                    state: representative.state.clone(),
                    controlled,
                    lifecycle: Vec::new(),
                    active,
                    waiting,
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

    async fn instantiate_worker(
        &mut self,
        loader: &mut NativeResourceLoader,
        script_url: String,
        scope: String,
        is_module: bool,
        source: String,
        import_script_counts: BTreeMap<String, usize>,
        module_graph: Option<NativeWorkerModuleGraph>,
        worker_referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<NativeServiceWorker, NativeEngineError> {
        let worker_id = self.next_worker_id()?;
        let mut runtime = NativeJavaScriptRuntime::new_with_context_id(format!(
            "glass-service-worker-{worker_id}"
        ))?;
        runtime.set_worker_global_referrer_policy(worker_referrer_policy);
        let module_name = if is_module {
            let graph = module_graph.ok_or_else(|| {
                NativeEngineError::invalid(
                    "service worker module graph",
                    "must be present for a module worker",
                )
            })?;
            runtime.set_module_sources(graph.sources);
            runtime.set_module_base_urls(graph.base_urls);
            Some(graph.root_name)
        } else {
            None
        };
        let mut worker = NativeServiceWorker {
            id: worker_id,
            script_url,
            scope,
            is_module,
            runtime,
            import_script_counts,
            skip_waiting_requested: false,
            clients_claim_requested: false,
            fetch_upload_connections: BTreeMap::new(),
        };
        self.set_worker_client_view(&worker)?;
        let initial = if let Some(module_name) = module_name.as_deref() {
            worker.runtime.evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                Some(module_name),
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
        let client_messages = settle_service_worker_cache_event(
            &mut worker,
            initial,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
        )
        .await?;
        self.enqueue_client_messages(client_messages)?;
        Ok(worker)
    }

    async fn restore_worker(
        &mut self,
        loader: &mut NativeResourceLoader,
        script_url: &str,
        scope: &str,
        worker_type: &str,
    ) -> Result<NativeServiceWorker, NativeEngineError> {
        let resource = loader
            .load_worker_async(script_url, script_url, MAX_NATIVE_SCRIPT_BYTES)
            .await?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "service worker restoration".into(),
                reason: "persisted service worker script was blocked or unavailable".into(),
            })?;
        let is_module = worker_type.eq_ignore_ascii_case("module");
        let root_request_url = resolve_module_request_url(script_url, script_url)?;
        let (source, import_script_counts, module_graph, worker_referrer_policy) =
            load_service_worker_source(
                loader,
                script_url,
                root_request_url,
                resource.clone(),
                is_module,
            )
            .await?;
        self.instantiate_worker(
            loader,
            resource.url,
            scope.to_owned(),
            is_module,
            source,
            import_script_counts,
            module_graph,
            worker_referrer_policy,
        )
        .await
    }

    async fn settle_worker_install(
        &mut self,
        worker: &mut NativeServiceWorker,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.set_worker_client_view(worker)?;
        let install = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "install",
            worker.is_module,
        )?;
        let client_messages = settle_service_worker_cache_event(
            worker,
            install,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
        )
        .await?;
        self.enqueue_client_messages(client_messages)
    }

    async fn settle_worker_activate(
        &mut self,
        worker: &mut NativeServiceWorker,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.set_worker_client_view(worker)?;
        let activate = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "activate",
            worker.is_module,
        )?;
        let client_messages = settle_service_worker_cache_event(
            worker,
            activate,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
        )
        .await?;
        self.enqueue_client_messages(client_messages)
    }

    fn commit_activated_worker(
        &mut self,
        scope: &str,
        worker: NativeServiceWorker,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        if let Some(previous_id) = self.registrations.get(scope).map(|previous| previous.id) {
            self.remove_worker_routes(previous_id);
        }
        if let Some(previous_waiting) = self.waiting_workers.remove(scope)
            && previous_waiting.id != worker.id
        {
            self.remove_worker_routes(previous_waiting.id);
        }
        let clients_claim_requested = worker.clients_claim_requested;
        let script_url = worker.script_url.clone();
        let active = worker_state(&worker, "activated");
        self.remember_registration(&worker);
        self.registrations.insert(scope.to_owned(), worker);
        if clients_claim_requested {
            // `clients.claim()` affects uncontrolled clients only after the
            // activating worker has become active.
            self.claim_current_client(scope)?;
        }
        Ok(NativeServiceWorkerRegistrationState {
            script_url,
            scope: scope.to_owned(),
            state: "activated".into(),
            controlled: self.current_client_scope.as_deref() == Some(scope),
            lifecycle: completed_lifecycle_states(),
            active: Some(active),
            waiting: None,
        })
    }

    async fn install_worker(
        &mut self,
        mut worker: NativeServiceWorker,
        loader: &mut NativeResourceLoader,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let scope = worker.scope.clone();
        self.settle_worker_install(&mut worker, loader).await?;
        if worker.skip_waiting_requested || !self.registrations.contains_key(&scope) {
            self.settle_worker_activate(&mut worker, loader).await?;
            return self.commit_activated_worker(&scope, worker);
        }
        if let Some(previous_waiting) = self.waiting_workers.insert(scope.clone(), worker) {
            self.remove_worker_routes(previous_waiting.id);
        }
        let waiting = self
            .waiting_workers
            .get(&scope)
            .map(|worker| NativeServiceWorkerWorkerProfile {
                script_url: worker.script_url.clone(),
                worker_type: worker_type_name(worker),
            })
            .expect("service worker waiting candidate was inserted");
        self.remember_waiting_registration(&scope, waiting);
        registration_state_for_workers(
            &scope,
            self.registrations.get(&scope),
            self.waiting_workers.get(&scope),
            self.current_client_scope.as_deref() == Some(scope.as_str()),
            waiting_lifecycle_states(),
        )
    }

    pub(crate) async fn update(
        &mut self,
        scope: &str,
        owner_url: &str,
        referrer_url: &str,
        referrer_policy: NativeFetchReferrerPolicy,
        loader: &mut NativeResourceLoader,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let owner = parse_network_url("service worker owner URL", owner_url)?;
        let referrer = parse_network_url("service worker referrer URL", referrer_url)?;
        if NativeOrigin::from_url(&owner)? != NativeOrigin::from_url(&referrer)? {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "service worker referrer must be same-origin with its client".into(),
            });
        }
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
                        worker_type: worker_type_name(worker),
                        navigation_preload_enabled: false,
                        navigation_preload_header_value: default_navigation_preload_header_value(),
                        waiting: None,
                    })
            })
            .or_else(|| {
                self.waiting_workers.get(scope).map(|worker| {
                    NativeServiceWorkerRegistrationProfile {
                        script_url: worker.script_url.clone(),
                        scope: worker.scope.clone(),
                        worker_type: worker_type_name(worker),
                        navigation_preload_enabled: false,
                        navigation_preload_header_value: default_navigation_preload_header_value(),
                        waiting: None,
                    }
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
            .load_worker_async_with_referrer_policy(
                referrer_url,
                &profile.script_url,
                MAX_NATIVE_SCRIPT_BYTES,
                Some(referrer_policy),
            )
            .await?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "service worker update".into(),
                reason: "service worker script was blocked or unavailable".into(),
            })?;
        let root_request_url =
            resolve_module_request_url(&profile.script_url, &profile.script_url)?;
        let (source, import_script_counts, module_graph, worker_referrer_policy) =
            load_service_worker_source(
                loader,
                &profile.script_url,
                root_request_url,
                resource.clone(),
                is_module,
            )
            .await?;
        let worker = self
            .instantiate_worker(
                loader,
                resource.url,
                profile.scope.clone(),
                is_module,
                source,
                import_script_counts,
                module_graph,
                worker_referrer_policy,
            )
            .await?;
        self.install_worker(worker, loader).await
    }

    fn remember_registration(&mut self, worker: &NativeServiceWorker) {
        let navigation_preload = self
            .registration_profiles
            .iter()
            .find(|profile| profile.scope == worker.scope)
            .map(|profile| {
                (
                    profile.navigation_preload_enabled,
                    profile.navigation_preload_header_value.clone(),
                )
            });
        self.registration_profiles
            .retain(|profile| profile.scope != worker.scope);
        let (navigation_preload_enabled, navigation_preload_header_value) = navigation_preload
            .unwrap_or_else(|| (false, default_navigation_preload_header_value()));
        self.registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: worker.script_url.clone(),
                scope: worker.scope.clone(),
                worker_type: worker_type_name(worker),
                navigation_preload_enabled,
                navigation_preload_header_value,
                waiting: None,
            });
        self.registration_profiles
            .sort_unstable_by(|left, right| left.scope.cmp(&right.scope));
        self.record_registration_change(&worker.scope);
    }

    fn remember_waiting_registration(
        &mut self,
        scope: &str,
        waiting: NativeServiceWorkerWorkerProfile,
    ) {
        if self
            .registration_profiles
            .iter()
            .any(|profile| profile.scope == scope)
        {
            if let Some(profile) = self
                .registration_profiles
                .iter_mut()
                .find(|profile| profile.scope == scope)
            {
                profile.waiting = Some(waiting);
            }
            self.record_registration_change(scope);
            return;
        }
        let Some(active) = self.registrations.get(scope) else {
            return;
        };
        self.registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: active.script_url.clone(),
                scope: active.scope.clone(),
                worker_type: worker_type_name(active),
                navigation_preload_enabled: false,
                navigation_preload_header_value: default_navigation_preload_header_value(),
                waiting: Some(waiting),
            });
        self.registration_profiles
            .sort_unstable_by(|left, right| left.scope.cmp(&right.scope));
        self.record_registration_change(scope);
    }

    fn record_registration_change(&mut self, scope: &str) {
        let profile = self
            .registration_profiles
            .iter()
            .find(|profile| profile.scope == scope)
            .cloned();
        self.registration_changes.insert(scope.to_owned(), profile);
    }

    pub(crate) async fn register(
        &mut self,
        owner_url: &str,
        script_url: &str,
        scope: &str,
        worker_type: &str,
        referrer_url: &str,
        referrer_policy: NativeFetchReferrerPolicy,
        loader: &mut NativeResourceLoader,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let owner = parse_network_url("service worker owner URL", owner_url)?;
        let referrer = parse_network_url("service worker referrer URL", referrer_url)?;
        let script = resolve_same_origin_url(&owner, "service worker script URL", script_url)?;
        let scope = resolve_same_origin_url(&owner, "service worker scope", scope)?;
        let owner_origin = NativeOrigin::from_url(&owner)?;
        if NativeOrigin::from_url(&referrer)? != owner_origin
            || NativeOrigin::from_url(&script)? != owner_origin
            || NativeOrigin::from_url(&scope)? != owner_origin
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason:
                    "service worker referrer, script, and scope must be same-origin with the page"
                        .into(),
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
        let known_scope = self.registrations.contains_key(&scope)
            || self.waiting_workers.contains_key(&scope)
            || self
                .registration_profiles
                .iter()
                .any(|profile| profile.scope == scope);
        let registration_scope_count = self
            .registrations
            .keys()
            .chain(self.waiting_workers.keys())
            .chain(
                self.registration_profiles
                    .iter()
                    .map(|profile| &profile.scope),
            )
            .cloned()
            .collect::<BTreeSet<_>>()
            .len();
        if !known_scope && registration_scope_count >= MAX_NATIVE_SERVICE_WORKERS {
            return Err(NativeEngineError::limit(
                "native service worker registrations",
                MAX_NATIVE_SERVICE_WORKERS,
                registration_scope_count.saturating_add(1),
            ));
        }
        let resource = loader
            .load_worker_async_with_referrer_policy(
                referrer_url,
                &script,
                MAX_NATIVE_SCRIPT_BYTES,
                Some(referrer_policy),
            )
            .await?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "service worker registration".into(),
                reason: "service worker script was blocked or unavailable".into(),
            })?;
        let root_request_url = resolve_module_request_url(owner_url, &script)?;
        let (source, import_script_counts, module_graph, worker_referrer_policy) =
            load_service_worker_source(
                loader,
                owner_url,
                root_request_url,
                resource.clone(),
                is_module,
            )
            .await?;
        let worker = self
            .instantiate_worker(
                loader,
                resource.url,
                scope.clone(),
                is_module,
                source,
                import_script_counts,
                module_graph,
                worker_referrer_policy,
            )
            .await?;
        self.install_worker(worker, loader).await
    }

    pub(crate) fn clear_page_message_port_routes(&mut self) {
        self.pending_message_port_messages.clear();
        self.pending_client_messages.clear();
        self.message_port_routes.clear();
    }

    /// Admit at most one due Service Worker timer turn at a page host
    /// boundary. Active and waiting workers share a rotating worker-id cursor
    /// so one continuously-ready registration cannot monopolize delivery to
    /// the page. The resulting host commands remain in the same bounded
    /// client/MessagePort queues as lifecycle and message events.
    pub(crate) async fn run_due_timers(
        &mut self,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        let mut workers = self
            .registrations
            .iter()
            .map(|(scope, worker)| (worker.id, false, scope.clone()))
            .chain(
                self.waiting_workers
                    .iter()
                    .map(|(scope, worker)| (worker.id, true, scope.clone())),
            )
            .collect::<Vec<_>>();
        workers.sort_unstable_by_key(|(worker_id, _, _)| *worker_id);

        let mut selected = None;
        let after_cursor = workers
            .iter()
            .filter(|(worker_id, _, _)| *worker_id > self.next_timer_worker_id);
        let before_cursor = workers
            .iter()
            .filter(|(worker_id, _, _)| *worker_id <= self.next_timer_worker_id);
        for (worker_id, waiting, scope) in after_cursor.chain(before_cursor) {
            let worker = if *waiting {
                self.waiting_workers.get(scope)
            } else {
                self.registrations.get(scope)
            };
            if let Some(worker) = worker
                && worker.runtime.next_worker_timer_delay_ms()? == Some(0)
            {
                selected = Some((*worker_id, *waiting, scope.clone()));
                break;
            }
        }
        let Some((worker_id, waiting, scope)) = selected else {
            return Ok(());
        };
        self.next_timer_worker_id = worker_id;

        let evaluation = if waiting {
            let worker = self.waiting_workers.get(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            worker.runtime.evaluate_service_worker_timers(
                worker.id,
                &worker.script_url,
                worker.is_module,
            )?
        } else {
            let worker = self.registrations.get(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            worker.runtime.evaluate_service_worker_timers(
                worker.id,
                &worker.script_url,
                worker.is_module,
            )?
        };

        let client_messages = if waiting {
            let worker = self.waiting_workers.get_mut(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            settle_service_worker_cache_event(
                worker,
                evaluation,
                loader,
                &mut self.cache_state,
                None,
                &mut self.pending_open_windows,
            )
            .await?
        } else {
            let worker = self.registrations.get_mut(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            settle_service_worker_cache_event(
                worker,
                evaluation,
                loader,
                &mut self.cache_state,
                None,
                &mut self.pending_open_windows,
            )
            .await?
        };
        self.enqueue_client_messages(client_messages)?;

        let message_port_commands = if waiting {
            self.waiting_workers
                .get(&scope)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default()
        } else {
            self.registrations
                .get(&scope)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default()
        };
        self.collect_message_port_commands(worker_id, message_port_commands)
    }

    pub(crate) fn take_message_port_messages(&mut self) -> Vec<NativeMessagePortPageMessage> {
        self.pending_message_port_messages.drain(..).collect()
    }

    pub(crate) fn take_client_messages(&mut self) -> Vec<NativeServiceWorkerClientMessage> {
        self.take_client_messages_matching(true)
    }

    pub(crate) fn take_external_client_messages(
        &mut self,
    ) -> Vec<NativeServiceWorkerClientMessage> {
        self.take_client_messages_matching(false)
    }

    fn take_client_messages_matching(
        &mut self,
        current: bool,
    ) -> Vec<NativeServiceWorkerClientMessage> {
        let current_client_id = self.current_client_id.clone();
        let mut selected = Vec::new();
        let mut retained = VecDeque::new();
        for message in self.pending_client_messages.drain(..) {
            if (message.client_id == current_client_id) == current {
                selected.push(message);
            } else {
                retained.push_back(message);
            }
        }
        self.pending_client_messages = retained;
        selected
    }

    pub(crate) fn take_open_windows(&mut self) -> Vec<NativeServiceWorkerOpenWindowRequest> {
        self.pending_open_windows
            .iter()
            .filter_map(|request| {
                self.announced_open_windows
                    .insert((request.worker_id, request.request_id))
                    .then(|| request.clone())
            })
            .collect()
    }

    pub(crate) fn take_completed_fetch(&mut self) -> Option<NativeServiceWorkerFetchCompletion> {
        self.completed_fetches.pop_front()
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
        let removed_active = self.registrations.remove(scope);
        let had_active = removed_active.is_some();
        if let Some(worker) = removed_active {
            self.remove_worker_routes(worker.id);
        }
        let removed_waiting = self.waiting_workers.remove(scope);
        let had_waiting = removed_waiting.is_some();
        if let Some(worker) = removed_waiting {
            self.remove_worker_routes(worker.id);
        }
        if self.current_client_scope.as_deref() == Some(scope) {
            self.current_client_scope = None;
        }
        let removed = removed_profile || had_active || had_waiting;
        if removed {
            self.record_registration_change(scope);
        }
        removed
    }

    pub(crate) async fn post_message(
        &mut self,
        loader: &mut NativeResourceLoader,
        scope: &str,
        source_origin: &str,
        data: &Value,
        transfer_ports: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        let Some(worker_id) = self.registrations.get(scope).map(|worker| worker.id) else {
            return Ok(());
        };
        validate_url_text("native service worker message origin", source_origin)?;
        let current_client_url = self.current_client_url.as_deref().ok_or_else(|| {
            NativeEngineError::invalid(
                "native service worker message source",
                "current client is unavailable",
            )
        })?;
        let current_client = Url::parse(without_fragment(current_client_url)).map_err(|_| {
            NativeEngineError::invalid(
                "native service worker message source",
                "current client URL is invalid",
            )
        })?;
        let source_origin_state = NativeOrigin::from_url(&current_client)?;
        let scope_url = Url::parse(without_fragment(scope)).map_err(|_| {
            NativeEngineError::invalid("native service worker scope", "URL is invalid")
        })?;
        if source_origin_state != NativeOrigin::from_url(&scope_url)?
            || source_origin_state.serialized() != source_origin
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "service worker message source must match the current same-origin client"
                    .into(),
            });
        }
        validate_message_port_transfers(transfer_ports)?;
        let clients = self.client_states_for_worker(scope)?;
        let source = clients
            .iter()
            .find(|client| {
                client.get("clientId").and_then(Value::as_str) == Some(&self.current_client_id)
            })
            .cloned()
            .filter(|client| {
                client.get("clientUrl").and_then(Value::as_str)
                    == Some(without_fragment(current_client_url))
            })
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker message source",
                    "current client is missing from the validated worker projection",
                )
            })?;
        self.register_page_transfers(worker_id, transfer_ports)?;
        let evaluation = {
            let worker = self
                .registrations
                .get(scope)
                .ok_or_else(|| NativeEngineError::invalid("service worker scope", "not found"))?;
            worker.runtime.set_service_worker_clients(clients);
            worker.runtime.dispatch_service_worker_message(
                worker.id,
                &worker.script_url,
                data,
                transfer_ports,
                &source,
                source_origin,
                worker.is_module,
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
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
        )
        .await
        {
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

    pub(crate) async fn resolve_open_window(
        &mut self,
        loader: &mut NativeResourceLoader,
        worker_id: u32,
        request_id: u32,
        payload: &Value,
    ) -> Result<bool, NativeEngineError> {
        if worker_id == 0 || request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker openWindow resolution",
                "request and worker ids must be positive",
            ));
        }
        let request_index = self
            .pending_open_windows
            .iter()
            .position(|request| request.worker_id == worker_id && request.request_id == request_id)
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker openWindow resolution",
                    "request is unknown or was already resolved",
                )
            })?;
        let request = self
            .pending_open_windows
            .remove(request_index)
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker openWindow resolution",
                    "request queue changed while resolving",
                )
            })?;
        self.announced_open_windows
            .remove(&(request.worker_id, request.request_id));
        let scope = self
            .registrations
            .iter()
            .find_map(|(scope, worker)| (worker.id == request.worker_id).then_some(scope.clone()))
            .or_else(|| {
                self.waiting_workers.iter().find_map(|(scope, worker)| {
                    (worker.id == request.worker_id).then_some(scope.clone())
                })
            })
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker openWindow resolution",
                    "worker is no longer registered",
                )
            })?;
        let current_client_id =
            (!self.current_client_id.is_empty()).then(|| self.current_client_id.clone());
        let continuation = self.pending_fetches.remove(&(worker_id, request_id));
        let (evaluation, worker_is_waiting) = if let Some(worker) = self.registrations.get(&scope) {
            (
                worker.runtime.resolve_service_worker_open_window(
                    worker.id,
                    &worker.script_url,
                    request.request_id,
                    payload,
                    worker.is_module,
                )?,
                false,
            )
        } else if let Some(worker) = self.waiting_workers.get(&scope) {
            (
                worker.runtime.resolve_service_worker_open_window(
                    worker.id,
                    &worker.script_url,
                    request.request_id,
                    payload,
                    worker.is_module,
                )?,
                true,
            )
        } else {
            return Err(NativeEngineError::invalid(
                "native service worker openWindow resolution",
                "worker is no longer registered",
            ));
        };
        if let Some(continuation) = continuation {
            let fallback_url = continuation.fallback_url.clone();
            let mut pending = continuation.pending;
            pending.extend(evaluation.commands);
            let evaluation = NativeScriptEvaluation {
                value: continuation.value,
                commands: pending.into_iter().collect(),
                top_level_await_pending: continuation.awaiting
                    || evaluation.top_level_await_pending,
                worker_script_error: None,
            };
            let settlement = if worker_is_waiting {
                let worker = self
                    .waiting_workers
                    .get_mut(&scope)
                    .expect("waiting service worker was retained");
                settle_service_worker_fetch(
                    worker,
                    loader,
                    evaluation,
                    &mut self.cache_state,
                    current_client_id.as_deref().unwrap_or_default(),
                    &fallback_url,
                    &mut self.pending_open_windows,
                )
                .await?
            } else {
                let worker = self
                    .registrations
                    .get_mut(&scope)
                    .expect("active service worker was retained");
                settle_service_worker_fetch(
                    worker,
                    loader,
                    evaluation,
                    &mut self.cache_state,
                    current_client_id.as_deref().unwrap_or_default(),
                    &fallback_url,
                    &mut self.pending_open_windows,
                )
                .await?
            };
            match settlement {
                NativeServiceWorkerFetchSettlement::Complete {
                    value,
                    client_messages,
                } => {
                    self.enqueue_client_messages(client_messages)?;
                    let response = if value.get("handled").and_then(Value::as_bool) == Some(true) {
                        let response =
                            value
                                .get("response")
                                .ok_or_else(|| NativeEngineError::Worker {
                                    operation: "decode service worker response".into(),
                                    reason: "service worker returned no response envelope".into(),
                                })?;
                        Some(decode_service_worker_response(response, &fallback_url)?)
                    } else {
                        None
                    };
                    if self.completed_fetches.len() >= MAX_NATIVE_EFFECTS {
                        return Err(NativeEngineError::limit(
                            "native completed service worker fetches",
                            MAX_NATIVE_EFFECTS,
                            self.completed_fetches.len().saturating_add(1),
                        ));
                    }
                    self.completed_fetches
                        .push_back(NativeServiceWorkerFetchCompletion {
                            worker_id,
                            response,
                        });
                    let message_port_commands = if worker_is_waiting {
                        self.waiting_workers
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    } else {
                        self.registrations
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    };
                    self.collect_message_port_commands(worker_id, message_port_commands)?;
                    Ok(true)
                }
                NativeServiceWorkerFetchSettlement::Suspended {
                    continuation,
                    client_messages,
                } => {
                    self.enqueue_client_messages(client_messages)?;
                    let key = (continuation.worker_id, continuation.open_window_request_id);
                    if self.pending_fetches.insert(key, continuation).is_some() {
                        return Err(NativeEngineError::Worker {
                            operation: "service worker fetch event".into(),
                            reason: "service worker fetch continuation identifier collided".into(),
                        });
                    }
                    let message_port_commands = if worker_is_waiting {
                        self.waiting_workers
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    } else {
                        self.registrations
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    };
                    self.collect_message_port_commands(worker_id, message_port_commands)?;
                    Ok(false)
                }
            }
        } else {
            let client_messages = if worker_is_waiting {
                let worker = self
                    .waiting_workers
                    .get_mut(&scope)
                    .expect("waiting service worker was retained");
                settle_service_worker_cache_event(
                    worker,
                    evaluation,
                    loader,
                    &mut self.cache_state,
                    current_client_id.as_deref(),
                    &mut self.pending_open_windows,
                )
                .await?
            } else {
                let worker = self
                    .registrations
                    .get_mut(&scope)
                    .expect("active service worker was retained");
                settle_service_worker_cache_event(
                    worker,
                    evaluation,
                    loader,
                    &mut self.cache_state,
                    current_client_id.as_deref(),
                    &mut self.pending_open_windows,
                )
                .await?
            };
            self.enqueue_client_messages(client_messages)?;
            let message_port_commands = if worker_is_waiting {
                self.waiting_workers
                    .get(&scope)
                    .map(|worker| worker.runtime.take_message_port_commands())
                    .unwrap_or_default()
            } else {
                self.registrations
                    .get(&scope)
                    .map(|worker| worker.runtime.take_message_port_commands())
                    .unwrap_or_default()
            };
            self.collect_message_port_commands(worker_id, message_port_commands)?;
            Ok(false)
        }
    }

    pub(crate) async fn apply_page_message_port_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        if commands.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native page MessagePort commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len(),
            ));
        }
        for command in commands {
            let (bridge_key, data, transfer_ports, object_urls) = match command {
                NativeScriptCommand::MessagePortClose {
                    bridge_key,
                    worker_id: None,
                } => {
                    validate_service_worker_message_port_bridge_key(&bridge_key)?;
                    let Some(worker_id) = self.message_port_routes.get(&bridge_key).copied() else {
                        continue;
                    };
                    let Some(scope) = self.registrations.iter().find_map(|(scope, worker)| {
                        (worker.id == worker_id).then_some(scope.clone())
                    }) else {
                        self.retire_message_port_route(&bridge_key);
                        continue;
                    };
                    if self.retire_message_port_route(&bridge_key).is_none() {
                        continue;
                    }
                    let evaluation = {
                        let worker = self.registrations.get(&scope).ok_or_else(|| {
                            NativeEngineError::invalid("service worker scope", "not found")
                        })?;
                        worker.runtime.dispatch_service_worker_message_port_close(
                            worker.id,
                            &worker.script_url,
                            &bridge_key,
                            worker.is_module,
                        )
                    }?;
                    let message_port_commands = self
                        .registrations
                        .get(&scope)
                        .map(|worker| worker.runtime.take_message_port_commands())
                        .unwrap_or_default();
                    if let Err(error) =
                        self.collect_message_port_commands(worker_id, message_port_commands)
                    {
                        return Err(error);
                    }
                    let client_messages = settle_service_worker_cache_event(
                        self.registrations
                            .get_mut(&scope)
                            .expect("service worker registration was retained"),
                        evaluation,
                        loader,
                        &mut self.cache_state,
                        None,
                        &mut self.pending_open_windows,
                    )
                    .await?;
                    self.enqueue_client_messages(client_messages)?;
                    continue;
                }
                NativeScriptCommand::MessagePortPostMessage {
                    bridge_key,
                    data,
                    worker_id: None,
                    transfer_ports,
                    object_urls,
                } => (bridge_key, data, transfer_ports, object_urls),
                _ => {
                    return Err(NativeEngineError::invalid(
                        "native page MessagePort command",
                        "command did not originate from the page realm",
                    ));
                }
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
                    &object_urls,
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
                loader,
                &mut self.cache_state,
                None,
                &mut self.pending_open_windows,
            )
            .await
            {
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
        referrer: Option<&str>,
        referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<NativeServiceWorkerNavigationOutcome, NativeEngineError> {
        let target = parse_network_url(
            "service worker navigation URL",
            without_fragment(&navigation.url),
        )?;
        self.activate_waiting_for_navigation(loader, &target)
            .await?;
        let navigation_preload = self
            .navigation_preload_configuration(&target, navigation.method.as_str())?
            .map(|(scope, header_value)| {
                let request_id = self.allocate_navigation_preload_request_id()?;
                Ok(NativeServiceWorkerNavigationPreloadRequest {
                    request_id,
                    scope,
                    header_value,
                })
            })
            .transpose()?;
        let request_referrer = match referrer {
            Some(source) => Some(
                referrer_for_navigation_with_policy(source, &navigation.url, referrer_policy)?
                    .unwrap_or_default(),
            ),
            None => None,
        };
        let outcome = self
            .intercept_fetch(
                loader,
                &navigation.url,
                &navigation.url,
                navigation.method.as_str(),
                BTreeMap::new(),
                navigation.body.clone(),
                navigation.body_content_type.clone(),
                NativeCorsMode::Navigation,
                NativeFetchRedirectMode::Follow,
                None,
                true,
                None,
                request_referrer.as_deref(),
                Some(referrer.unwrap_or_default()),
                Some(referrer_policy.as_str()),
                "document",
                navigation_preload,
                referrer,
            )
            .await?;
        let response = match outcome {
            NativeServiceWorkerFetchOutcome::Handled(response) => response,
            NativeServiceWorkerFetchOutcome::NotHandled => {
                return Ok(NativeServiceWorkerNavigationOutcome::NotHandled);
            }
            NativeServiceWorkerFetchOutcome::Suspended => {
                return Ok(NativeServiceWorkerNavigationOutcome::Suspended);
            }
        };
        let url = if response.url.is_empty() {
            without_fragment(&navigation.url).to_owned()
        } else {
            response.url.clone()
        };
        let parsed = parse_network_url("service worker navigation response URL", &url)?;
        loader.set_document_content_security_policy_from_pairs(&url, &response.headers)?;
        Ok(NativeServiceWorkerNavigationOutcome::Handled(
            NativeResource {
                url,
                origin: NativeOrigin::from_url(&parsed)?,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            },
        ))
    }

    fn navigation_preload_configuration(
        &self,
        target: &Url,
        method: &str,
    ) -> Result<Option<(String, String)>, NativeEngineError> {
        if method != "GET" {
            return Ok(None);
        }
        let Some(scope) = self.matching_scope(target)? else {
            return Ok(None);
        };
        let Some(profile) = self
            .registration_profiles
            .iter()
            .find(|profile| profile.scope == scope)
        else {
            return Ok(None);
        };
        if !profile.navigation_preload_enabled {
            return Ok(None);
        }
        let Some(worker) = self.registrations.get(&scope) else {
            return Ok(None);
        };
        if !worker.runtime.has_service_worker_fetch_listener()? {
            return Ok(None);
        }
        Ok(Some((
            scope,
            profile.navigation_preload_header_value.clone(),
        )))
    }

    fn allocate_navigation_preload_request_id(&mut self) -> Result<u32, NativeEngineError> {
        let request_id = self.next_navigation_preload_request_id;
        if request_id == 0 {
            return Err(NativeEngineError::limit(
                "native ServiceWorker navigation preload request ids",
                u32::MAX as usize,
                u32::MAX as usize,
            ));
        }
        self.next_navigation_preload_request_id = request_id.checked_add(1).unwrap_or_default();
        Ok(request_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fetch_is_controlled(
        &self,
        document_url: &str,
        href: &str,
        destination: &str,
    ) -> Result<bool, NativeEngineError> {
        let owner = parse_network_url("service worker fetch owner URL", document_url)?;
        let target =
            resolve_same_origin_or_cross_origin_url(&owner, "service worker fetch URL", href)?;
        let scope = if destination == "document" {
            self.matching_scope(&target)?
        } else if self.current_client_is_controlled(document_url) {
            self.current_client_scope.clone()
        } else {
            None
        };
        Ok(scope.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn intercept_fetch(
        &mut self,
        loader: &mut NativeResourceLoader,
        document_url: &str,
        href: &str,
        method: &str,
        request_headers: BTreeMap<String, String>,
        body: Option<NativeRequestBody>,
        content_type: Option<String>,
        cors_mode: NativeCorsMode,
        redirect_mode: NativeFetchRedirectMode,
        _timeout: Option<Duration>,
        credentials: bool,
        credentials_mode: Option<NativeFetchCredentialsMode>,
        referrer: Option<&str>,
        referrer_url: Option<&str>,
        referrer_policy: Option<&str>,
        destination: &str,
        navigation_preload: Option<NativeServiceWorkerNavigationPreloadRequest>,
        navigation_referrer: Option<&str>,
    ) -> Result<NativeServiceWorkerFetchOutcome, NativeEngineError> {
        let owner = parse_network_url("service worker fetch owner URL", document_url)?;
        let target =
            resolve_same_origin_or_cross_origin_url(&owner, "service worker fetch URL", href)?;
        let is_navigation = destination == "document";
        let scope = if is_navigation {
            self.matching_scope(&target)?
        } else if self.current_client_is_controlled(document_url) {
            self.current_client_scope.clone()
        } else {
            None
        };
        let Some(scope) = scope else {
            return Ok(NativeServiceWorkerFetchOutcome::NotHandled);
        };
        if let Some(preload) = navigation_preload.as_ref()
            && (!is_navigation || method != "GET" || preload.scope != scope)
        {
            return Err(NativeEngineError::invalid(
                "native ServiceWorker navigation preload request",
                "must match an eligible GET navigation and its registration scope",
            ));
        }
        if !is_navigation {
            if destination == "font" {
                loader.enforce_service_worker_font_policy(&owner, &target)?;
            } else {
                loader.enforce_service_worker_connect_policy(&owner, &target)?;
            }
        }
        let client_id = self.current_client_id_for(document_url);
        let controlled = self.current_client_is_controlled(document_url);
        let clients = self.client_states_for_worker(&scope)?;
        let current_client = self
            .client_states
            .iter()
            .find(|state| state.id == client_id)
            .cloned();
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
        let payload = json!({
            "url": without_fragment(target.as_str()),
            "method": method,
            "headers": headers,
            "body": body_text,
            "bodyBase64": body_base64,
            "bodyNull": body_null,
            "contentType": content_type,
            "mode": if is_navigation {
                "navigate"
            } else {
                cors_mode_text(cors_mode)
            },
            "redirect": redirect_mode_text(redirect_mode),
            "credentials": credentials,
            "credentialsMode": credentials_mode.map(NativeFetchCredentialsMode::as_str),
            "referrer": referrer.unwrap_or("about:client"),
            "referrerUrl": referrer_url.unwrap_or(client_url),
            "referrerPolicy": referrer_policy.unwrap_or(""),
            "destination": destination,
            "clientId": client_id,
            "clientUrl": client_url,
            "clientType": current_client
                .as_ref()
                .map(|state| state.client_type.as_str())
                .unwrap_or("window"),
            "frameType": current_client
                .as_ref()
                .map(|state| state.frame_type.as_str())
                .unwrap_or("top-level"),
            "visibilityState": current_client
                .as_ref()
                .map(|state| state.visibility_state.as_str())
                .unwrap_or("visible"),
            "focused": current_client.as_ref().is_some_and(|state| state.focused),
            "controlled": controlled,
            "clients": clients,
            "navigationPreloadRequestId": navigation_preload
                .as_ref()
                .map(|preload| preload.request_id),
        });
        let Some(worker) = self.registrations.get_mut(&scope) else {
            return Ok(NativeServiceWorkerFetchOutcome::NotHandled);
        };
        worker.runtime.set_service_worker_clients(
            payload
                .get("clients")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        let mut preload_response = None;
        let evaluation = if let Some(preload) = navigation_preload.as_ref() {
            let worker_id = worker.id;
            let worker_url = worker.script_url.clone();
            let is_module = worker.is_module;
            let preload_referrer_policy = referrer_policy
                .map(NativeFetchReferrerPolicy::parse)
                .transpose()?
                .ok_or_else(|| {
                    NativeEngineError::invalid(
                        "native ServiceWorker navigation preload policy",
                        "must be present for an eligible preload",
                    )
                })?;
            let mut preload_request = Box::pin(loader.fetch_navigation_preload_async(
                target.as_str(),
                navigation_referrer,
                preload_referrer_policy,
                &preload.header_value,
            ));
            let mut preload_result = tokio::select! {
                biased;
                result = &mut preload_request => Some(result),
                _ = tokio::task::yield_now() => None,
            };
            let mut evaluation = worker.runtime.evaluate_service_worker_fetch(
                worker_id,
                &worker_url,
                &payload,
                is_module,
            )?;
            let fetch_handler_responded = !evaluation.top_level_await_pending
                && evaluation.value.get("handled").and_then(Value::as_bool) == Some(true);
            if fetch_handler_responded && preload_result.is_none() {
                preload_result = std::future::poll_fn(|context| {
                    std::task::Poll::Ready(match preload_request.as_mut().poll(context) {
                        std::task::Poll::Ready(result) => Some(result),
                        std::task::Poll::Pending => None,
                    })
                })
                .await;
            }
            let preload_result = match preload_result {
                Some(result) => Some(result),
                None if fetch_handler_responded => {
                    drop(preload_request);
                    None
                }
                None => Some(preload_request.await),
            };
            let resolve_payload = match &preload_result {
                Some(Ok(response)) => service_worker_fetch_payload(response.clone()),
                Some(Err(error)) => {
                    let message = error.to_string().chars().take(1024).collect::<String>();
                    json!({"error": message})
                }
                None => json!({
                    "error": "navigation preload was aborted after the ServiceWorker response settled"
                }),
            };
            let resolved = worker.runtime.resolve_service_worker_navigation_preload(
                worker_id,
                &worker_url,
                preload.request_id,
                &resolve_payload,
                is_module,
            )?;
            if evaluation
                .commands
                .len()
                .saturating_add(resolved.commands.len())
                > MAX_NATIVE_WORKER_MESSAGES
            {
                return Err(NativeEngineError::limit(
                    "native ServiceWorker navigation preload commands",
                    MAX_NATIVE_WORKER_MESSAGES,
                    evaluation
                        .commands
                        .len()
                        .saturating_add(resolved.commands.len()),
                ));
            }
            evaluation.commands.extend(resolved.commands);
            evaluation.top_level_await_pending |= resolved.top_level_await_pending;
            if evaluation.worker_script_error.is_none() {
                evaluation.worker_script_error = resolved.worker_script_error;
            }
            preload_response = preload_result.and_then(Result::ok);
            evaluation
        } else {
            worker.runtime.evaluate_service_worker_fetch(
                worker.id,
                &worker.script_url,
                &payload,
                worker.is_module,
            )?
        };
        let worker_id = worker.id;
        let settlement = settle_service_worker_fetch(
            worker,
            loader,
            evaluation,
            &mut self.cache_state,
            &client_id,
            target.as_str(),
            &mut self.pending_open_windows,
        )
        .await?;
        let (value, suspended, client_messages) = match settlement {
            NativeServiceWorkerFetchSettlement::Complete {
                value,
                client_messages,
            } => (value, None, client_messages),
            NativeServiceWorkerFetchSettlement::Suspended {
                continuation,
                client_messages,
            } => (Value::Null, Some(continuation), client_messages),
        };
        let clients_claim_requested = worker.clients_claim_requested;
        self.enqueue_client_messages(client_messages)?;
        let message_port_commands = self
            .registrations
            .get(&scope)
            .map(|worker| worker.runtime.take_message_port_commands())
            .unwrap_or_default();
        self.collect_message_port_commands(worker_id, message_port_commands)?;
        if clients_claim_requested {
            self.claim_current_client(&scope)?;
        }
        if let Some(continuation) = suspended {
            let key = (continuation.worker_id, continuation.open_window_request_id);
            if self.pending_fetches.insert(key, continuation).is_some() {
                return Err(NativeEngineError::Worker {
                    operation: "service worker fetch event".into(),
                    reason: "service worker fetch continuation identifier collided".into(),
                });
            }
            return Ok(NativeServiceWorkerFetchOutcome::Suspended);
        }
        if value.get("handled").and_then(Value::as_bool) != Some(true) {
            if let Some(response) = preload_response {
                return Ok(NativeServiceWorkerFetchOutcome::Handled(response));
            }
            return Ok(NativeServiceWorkerFetchOutcome::NotHandled);
        }
        let response = value
            .get("response")
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker returned no response envelope".into(),
            })?;
        let response = decode_service_worker_response(response, target.as_str())?;
        if !is_navigation {
            if destination == "font" {
                loader.report_service_worker_font_policy(&owner, &target);
            } else {
                loader.report_service_worker_connect_policy(&owner, &target);
            }
        }
        Ok(NativeServiceWorkerFetchOutcome::Handled(response))
    }

    fn matching_scope(&self, target: &Url) -> Result<Option<String>, NativeEngineError> {
        let target_origin = NativeOrigin::from_url(target)?;
        Ok(self
            .registrations
            .values()
            .filter_map(|worker| {
                let scope = Url::parse(without_fragment(&worker.scope)).ok()?;
                if NativeOrigin::from_url(&scope).ok()? != target_origin
                    || !service_worker_scope_matches(scope.path(), target.path())
                {
                    return None;
                }
                Some((scope.path().len(), worker.scope.clone()))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, scope)| scope))
    }

    fn matching_waiting_scope(&self, target: &Url) -> Result<Option<String>, NativeEngineError> {
        let target_origin = NativeOrigin::from_url(target)?;
        Ok(self
            .waiting_workers
            .values()
            .filter_map(|worker| {
                let scope = Url::parse(without_fragment(&worker.scope)).ok()?;
                if NativeOrigin::from_url(&scope).ok()? != target_origin
                    || !service_worker_scope_matches(scope.path(), target.path())
                {
                    return None;
                }
                Some((scope.path().len(), worker.scope.clone()))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, scope)| scope))
    }

    async fn activate_waiting_for_navigation(
        &mut self,
        loader: &mut NativeResourceLoader,
        target: &Url,
    ) -> Result<(), NativeEngineError> {
        let Some(scope) = self.matching_waiting_scope(target)? else {
            return Ok(());
        };
        let Some(mut worker) = self.waiting_workers.remove(&scope) else {
            return Ok(());
        };
        if let Err(error) = self.settle_worker_activate(&mut worker, loader).await {
            self.waiting_workers.insert(scope, worker);
            return Err(error);
        }
        self.commit_activated_worker(&scope, worker)?;
        Ok(())
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
        self.validate_transfer_registration(transfers, 0)?;
        for transfer in transfers {
            self.message_port_routes
                .insert(transfer.bridge_key.clone(), worker_id);
        }
        Ok(())
    }

    fn validate_transfer_registration(
        &self,
        transfers: &[NativeMessagePortTransfer],
        additional_events: usize,
    ) -> Result<(), NativeEngineError> {
        if transfers.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Service Worker MessagePort transfers",
                MAX_NATIVE_WORKER_MESSAGES,
                transfers.len(),
            ));
        }
        validate_message_port_transfers(transfers)?;
        let mut new_routes = 0usize;
        for transfer in transfers {
            if self.message_port_routes.contains_key(&transfer.bridge_key) {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort transfer",
                    "bridge key was already transferred",
                ));
            }
            new_routes = new_routes.saturating_add(1);
        }
        let occupied = self
            .message_port_routes
            .len()
            .saturating_add(self.pending_message_port_messages.len())
            .saturating_add(new_routes)
            .saturating_add(additional_events);
        if occupied > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Service Worker MessagePort routes and queued events",
                MAX_NATIVE_WORKER_MESSAGES,
                occupied,
            ));
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
            .retain(|message| message.close || live_bridge_keys.contains(&message.bridge_key));
    }

    fn retire_message_port_route(&mut self, bridge_key: &str) -> Option<u32> {
        let worker_id = self.message_port_routes.remove(bridge_key)?;
        self.pending_message_port_messages
            .retain(|message| message.bridge_key != bridge_key);
        Some(worker_id)
    }

    fn close_worker_message_port(
        &mut self,
        worker_id: u32,
        bridge_key: &str,
    ) -> Result<(), NativeEngineError> {
        validate_service_worker_message_port_bridge_key(bridge_key)?;
        let Some(route_worker_id) = self.message_port_routes.get(bridge_key).copied() else {
            return Ok(());
        };
        if route_worker_id != worker_id {
            return Err(NativeEngineError::invalid(
                "native service-worker MessagePort route",
                "worker id does not own the MessagePort bridge",
            ));
        }
        if self.pending_message_port_messages.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Service Worker MessagePort close events",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_message_port_messages.len().saturating_add(1),
            ));
        }
        if self.retire_message_port_route(bridge_key).is_none() {
            return Ok(());
        }
        self.pending_message_port_messages
            .push_back(NativeMessagePortPageMessage {
                bridge_key: bridge_key.to_owned(),
                data: Value::Null,
                close: true,
                transfer_ports: Vec::new(),
                object_urls: Vec::new(),
            });
        Ok(())
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
            if let NativeScriptCommand::MessagePortClose {
                bridge_key,
                worker_id: Some(command_worker_id),
            } = &command
            {
                if *command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native service-worker MessagePort command",
                        "service worker command owner is invalid",
                    ));
                }
                self.close_worker_message_port(worker_id, bridge_key)?;
                continue;
            }
            let NativeScriptCommand::MessagePortPostMessage {
                bridge_key,
                data,
                worker_id: Some(command_worker_id),
                transfer_ports,
                object_urls,
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
            validate_service_worker_message_port_bridge_key(&bridge_key)?;
            validate_native_object_url_transfers(&object_urls)?;
            validate_native_message_payload(
                &serde_json::json!({
                    "data": &data,
                    "transfer_ports": &transfer_ports,
                    "object_urls": &object_urls,
                }),
                "native service-worker MessagePort event",
            )?;
            self.validate_transfer_registration(&transfer_ports, 1)?;
            self.register_worker_transfers(worker_id, &transfer_ports)?;
            self.pending_message_port_messages
                .push_back(NativeMessagePortPageMessage {
                    bridge_key,
                    data,
                    close: false,
                    transfer_ports,
                    object_urls,
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
            .retain(|message| message.close || live_bridge_keys.contains(&message.bridge_key));
        self.pending_open_windows
            .retain(|request| request.worker_id != worker_id);
        self.pending_fetches
            .retain(|(pending_worker_id, _), _| *pending_worker_id != worker_id);
        self.completed_fetches
            .retain(|completion| completion.worker_id != worker_id);
        self.announced_open_windows
            .retain(|(request_worker_id, _)| *request_worker_id != worker_id);
    }
}

fn validate_service_worker_message_port_bridge_key(
    bridge_key: &str,
) -> Result<(), NativeEngineError> {
    validate_url_text("native service worker MessagePort bridge key", bridge_key)?;
    if bridge_key.is_empty() {
        return Err(NativeEngineError::invalid(
            "native service worker MessagePort bridge key",
            "must not be empty",
        ));
    }
    if bridge_key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native service worker MessagePort bridge key",
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            bridge_key.len(),
        ));
    }
    Ok(())
}

fn service_worker_scope_matches(scope_path: &str, target_path: &str) -> bool {
    target_path == scope_path
        || (scope_path.ends_with('/') && target_path.starts_with(scope_path))
        || target_path
            .strip_prefix(scope_path)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

async fn settle_service_worker_cache_event(
    worker: &mut NativeServiceWorker,
    evaluation: NativeScriptEvaluation,
    loader: &mut NativeResourceLoader,
    cache_state: &mut NativeServiceWorkerCacheState,
    _current_client_id: Option<&str>,
    pending_open_windows: &mut VecDeque<NativeServiceWorkerOpenWindowRequest>,
) -> Result<Vec<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let mut pending = VecDeque::from(evaluation.commands);
    let mut value = evaluation.value;
    let mut awaiting = evaluation.top_level_await_pending;
    let mut open_window_pending = false;
    let mut client_messages = Vec::new();
    let mut turns = 0usize;
    if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
        value = resolved_value;
        awaiting = false;
    }
    while let Some(command) = pending.pop_front() {
        turns = turns.saturating_add(1);
        if turns > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "service worker cache event turns",
                MAX_NATIVE_MODULE_IMPORTS,
                turns,
            ));
        }
        if apply_service_worker_lifecycle_command(worker, &command)? {
            continue;
        }
        if let Some(message) = service_worker_client_message_command(worker.id, command.clone())? {
            client_messages.push(message);
            continue;
        }
        if let Some(request) = service_worker_open_window_command(worker, command.clone())? {
            if pending_open_windows.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native service worker openWindow requests",
                    MAX_NATIVE_EFFECTS,
                    pending_open_windows.len().saturating_add(1),
                ));
            }
            pending_open_windows.push_back(request);
            open_window_pending = true;
            continue;
        }
        if resolve_service_worker_fetch_command(
            worker,
            loader,
            command.clone(),
            &mut pending,
            &mut awaiting,
            &mut value,
        )
        .await?
        {
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
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            value = resolved_value;
            awaiting = false;
        }
    }
    if awaiting && worker.runtime.take_top_level_await_result()?.is_none() && !open_window_pending {
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
) -> Result<Option<NativeServiceWorkerClientMessage>, NativeEngineError> {
    let NativeScriptCommand::ServiceWorkerClientPostMessage {
        worker_id: command_worker_id,
        client_id,
        data,
        transfer_ports,
        object_urls,
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
    validate_native_object_url_transfers(&object_urls)?;
    validate_native_message_payload(
        &json!({
            "data": &data,
            "transfer_ports": &transfer_ports,
            "object_urls": &object_urls,
        }),
        "native service worker client message",
    )?;
    Ok(Some(NativeServiceWorkerClientMessage {
        worker_id,
        client_id,
        data,
        transfer_ports,
        object_urls,
    }))
}

fn service_worker_open_window_command(
    worker: &NativeServiceWorker,
    command: NativeScriptCommand,
) -> Result<Option<NativeServiceWorkerOpenWindowRequest>, NativeEngineError> {
    let NativeScriptCommand::ServiceWorkerOpenWindow {
        request_id,
        worker_id,
        url,
    } = command
    else {
        return Ok(None);
    };
    if request_id == 0 || worker_id == 0 || worker_id != worker.id {
        return Err(NativeEngineError::Worker {
            operation: "service worker openWindow request".into(),
            reason: "service worker openWindow request owner is invalid".into(),
        });
    }
    let target = parse_network_url("service worker openWindow URL", &url)?;
    let worker_url = parse_network_url("service worker URL", &worker.script_url)?;
    if NativeOrigin::from_url(&target)? != NativeOrigin::from_url(&worker_url)? {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "service worker openWindow URL must be same-origin".into(),
        });
    }
    Ok(Some(NativeServiceWorkerOpenWindowRequest {
        request_id,
        worker_id,
        url: without_fragment(target.as_str()).to_owned(),
        source_context_id: String::new(),
        source_frame_id: String::new(),
    }))
}

fn cancel_service_worker_fetch_upload(worker: &mut NativeServiceWorker, request_id: u32) {
    if let Some(connection) = worker.fetch_upload_connections.remove(&request_id) {
        let _ = connection
            .commands
            .try_send(NativeFetchUploadCommand::Cancel);
    }
}

fn process_service_worker_fetch_upload_command(
    worker: &mut NativeServiceWorker,
    request_id: u32,
    command: NativeScriptCommand,
) -> Result<bool, NativeEngineError> {
    let upload_command = matches!(
        &command,
        NativeScriptCommand::FetchUploadChunk { .. }
            | NativeScriptCommand::FetchUploadEnd { .. }
            | NativeScriptCommand::FetchUploadError { .. }
            | NativeScriptCommand::FetchUploadCancel { .. }
    );
    if !upload_command {
        return Ok(false);
    }
    match command {
        NativeScriptCommand::FetchUploadChunk {
            stream_id,
            data_base64,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            let connection = worker
                .fetch_upload_connections
                .get_mut(&request_id)
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "service worker fetch request upload chunk".into(),
                    reason: "service worker fetch request upload identifier is not active".into(),
                })?;
            if !connection.demand_pending {
                return Err(NativeEngineError::Network {
                    operation: "service worker fetch request upload chunk".into(),
                    reason: "service worker fetch request upload chunk arrived without demand"
                        .into(),
                });
            }
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_base64)
                .map_err(|_| {
                    NativeEngineError::invalid(
                        "service worker fetch request upload chunk",
                        "must be valid base64",
                    )
                })?;
            let next_total = connection.total_bytes.saturating_add(data.len());
            let next_chunks = connection.chunk_count.saturating_add(1);
            if next_total > MAX_NATIVE_FORM_BODY_BYTES
                || next_chunks > MAX_NATIVE_FETCH_UPLOAD_CHUNKS
            {
                let connection = worker
                    .fetch_upload_connections
                    .remove(&request_id)
                    .expect("service worker fetch upload connection was checked above");
                let message = if next_total > MAX_NATIVE_FORM_BODY_BYTES {
                    "native service worker fetch request upload body exceeds its limit"
                } else {
                    "native service worker fetch request upload chunk limit exceeded"
                };
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Error {
                        message: message.into(),
                    });
                return Ok(true);
            }
            connection.total_bytes = next_total;
            connection.chunk_count = next_chunks;
            connection.demand_pending = false;
            connection
                .commands
                .try_send(NativeFetchUploadCommand::Chunk { data })
                .map_err(|_| NativeEngineError::Network {
                    operation: "service worker fetch request upload chunk".into(),
                    reason: "service worker fetch request upload task is unavailable".into(),
                })?;
            Ok(true)
        }
        NativeScriptCommand::FetchUploadEnd {
            stream_id,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            let mut connection = worker
                .fetch_upload_connections
                .remove(&request_id)
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "service worker fetch request upload end".into(),
                    reason: "service worker fetch request upload identifier is not active".into(),
                })?;
            if !connection.demand_pending {
                return Err(NativeEngineError::Network {
                    operation: "service worker fetch request upload end".into(),
                    reason: "service worker fetch request upload ended without demand".into(),
                });
            }
            connection.demand_pending = false;
            connection
                .commands
                .try_send(NativeFetchUploadCommand::End)
                .map_err(|_| NativeEngineError::Network {
                    operation: "service worker fetch request upload end".into(),
                    reason: "service worker fetch request upload task is unavailable".into(),
                })?;
            Ok(true)
        }
        NativeScriptCommand::FetchUploadError {
            stream_id,
            message,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            let connection = worker
                .fetch_upload_connections
                .remove(&request_id)
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "service worker fetch request upload error".into(),
                    reason: "service worker fetch request upload identifier is not active".into(),
                })?;
            let message = message
                .chars()
                .take(crate::browser_backend::MAX_TEXT_BYTES)
                .collect();
            let _ = connection
                .commands
                .try_send(NativeFetchUploadCommand::Error { message });
            Ok(true)
        }
        NativeScriptCommand::FetchUploadCancel {
            stream_id,
            worker_id: Some(worker_id),
        } if worker_id == worker.id && stream_id == request_id => {
            cancel_service_worker_fetch_upload(worker, request_id);
            Ok(true)
        }
        _ => Err(NativeEngineError::invalid(
            "service worker fetch request upload command",
            "worker id or upload stream identifier is invalid",
        )),
    }
}

/// Drive a Service Worker-owned request body until its HTTP fetch has
/// completed. The Service Worker remains the sole stream producer: each
/// upload demand re-enters its serialized realm and any unrelated commands
/// emitted by that body callback return to the existing command queue.
async fn open_service_worker_fetch_upload(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    request_id: u32,
    href: String,
    method: NativeFetchMethod,
    headers: BTreeMap<String, String>,
    content_type: Option<String>,
    credentials: bool,
    credentials_mode: NativeFetchCredentialsMode,
    referrer_url: Option<String>,
    referrer_policy: Option<String>,
    cors_mode: NativeCorsMode,
    redirect_mode: NativeFetchRedirectMode,
    cache_mode: NativeFetchCacheMode,
    timeout: Option<Duration>,
    pending: &mut VecDeque<NativeScriptCommand>,
) -> Result<Result<NativeFetchResponse, NativeEngineError>, NativeEngineError> {
    if worker.fetch_upload_connections.contains_key(&request_id) {
        return Err(NativeEngineError::Network {
            operation: "service worker fetch request upload".into(),
            reason: "service worker fetch upload identifier is already active".into(),
        });
    }
    if worker.fetch_upload_connections.len() >= MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS {
        return Err(NativeEngineError::limit(
            "native service worker fetch request uploads",
            MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS,
            worker.fetch_upload_connections.len().saturating_add(1),
        ));
    }
    let (upload_connection, request_body) = spawn_native_fetch_upload_stream();
    worker
        .fetch_upload_connections
        .insert(request_id, upload_connection);
    let worker_url = worker.script_url.clone();
    let task_loader = loader.clone();
    let task = tokio::spawn(async move {
        let mut task_loader = task_loader;
        let result = task_loader
            .fetch_request_with_body_async(
                NativeFetchRequest {
                    document_url: &worker_url,
                    href: &href,
                    method,
                    body: None,
                    content_type,
                    request_headers: headers,
                    credentials,
                    credentials_mode: Some(credentials_mode),
                    referrer_url,
                    referrer_policy,
                    cors_mode,
                    redirect_mode,
                    cache_mode,
                    timeout,
                    max_response_bytes: None,
                },
                request_body,
            )
            .await;
        (result, task_loader)
    });
    loop {
        if task.is_finished() {
            let (result, task_loader) = task.await.map_err(|_| NativeEngineError::Worker {
                operation: "service worker fetch request upload task".into(),
                reason: "native service worker fetch upload task terminated unexpectedly".into(),
            })?;
            cancel_service_worker_fetch_upload(worker, request_id);
            loader.merge_fetch_task_state(task_loader)?;
            return Ok(result);
        }

        let mut disconnected = false;
        let demand = if let Some(connection) = worker.fetch_upload_connections.get_mut(&request_id)
        {
            match connection.events.try_recv() {
                Ok(NativeFetchUploadEvent::Demand) => {
                    connection.demand_pending = true;
                    true
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => false,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    disconnected = true;
                    false
                }
            }
        } else {
            false
        };
        if disconnected {
            task.abort();
            cancel_service_worker_fetch_upload(worker, request_id);
            return Err(NativeEngineError::Network {
                operation: "service worker fetch request upload".into(),
                reason: "service worker fetch upload task is unavailable".into(),
            });
        }
        if demand {
            let evaluation = worker.runtime.evaluate_service_worker_fetch_upload_event(
                worker.id,
                &worker.script_url,
                request_id,
                &json!({"type": "demand"}),
                worker.is_module,
            )?;
            for command in evaluation.commands {
                if !process_service_worker_fetch_upload_command(
                    worker,
                    request_id,
                    command.clone(),
                )? {
                    pending.push_back(command);
                }
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}

async fn resolve_service_worker_fetch_command(
    worker: &mut NativeServiceWorker,
    loader: &mut NativeResourceLoader,
    command: NativeScriptCommand,
    pending: &mut VecDeque<NativeScriptCommand>,
    awaiting: &mut bool,
    value: &mut Value,
) -> Result<bool, NativeEngineError> {
    let NativeScriptCommand::Fetch {
        request_id,
        worker_id,
        href,
        credentials,
        credentials_mode,
        referrer: _,
        referrer_url,
        referrer_policy,
        method,
        headers,
        body,
        body_base64,
        content_type,
        mode,
        redirect,
        cache,
        timeout_ms,
        upload_stream_id,
        destination,
        module_referrer,
        module_type,
    } = command
    else {
        return Ok(false);
    };
    let Some(worker_id) = worker_id else {
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch command owner is invalid".into(),
        });
    };
    if worker_id != worker.id || request_id == 0 {
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch command owner is invalid".into(),
        });
    }
    if destination.is_some() || module_referrer.is_some() || module_type.is_some() {
        return Err(NativeEngineError::invalid(
            "service worker fetch destination",
            "service worker fetch destinations and module metadata are not supported",
        ));
    }
    let request_body = if let Some(upload_stream_id) = upload_stream_id {
        if upload_stream_id != request_id {
            return Err(NativeEngineError::invalid(
                "service worker fetch request upload stream",
                "upload stream identifier must match its fetch request",
            ));
        }
        if body.is_some() || body_base64.is_some() {
            return Err(NativeEngineError::invalid(
                "service worker fetch request upload stream",
                "streaming fetch requests must not also carry a buffered body",
            ));
        }
        None
    } else {
        match body_base64 {
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
        }
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
    let method = NativeFetchMethod::from_fetch_method(&method)?;
    let cors_mode = parse_cors_mode(mode.as_deref().unwrap_or("same-origin"))?;
    let redirect_mode = parse_redirect_mode(redirect.as_deref().unwrap_or("follow"))?;
    let cache_mode = NativeFetchCacheMode::from_option(cache.as_deref())?;
    let credentials_mode = credentials_mode
        .as_deref()
        .map(NativeFetchCredentialsMode::parse)
        .transpose()?
        .unwrap_or(if credentials {
            NativeFetchCredentialsMode::Include
        } else {
            NativeFetchCredentialsMode::Omit
        });
    let response = if upload_stream_id.is_some() {
        open_service_worker_fetch_upload(
            worker,
            loader,
            request_id,
            href,
            method,
            headers,
            content_type,
            credentials,
            credentials_mode,
            referrer_url,
            referrer_policy,
            cors_mode,
            redirect_mode,
            cache_mode,
            timeout_ms.map(|value| Duration::from_millis(u64::from(value))),
            pending,
        )
        .await?
    } else {
        loader
            .fetch_request_with_headers_async(NativeFetchRequest {
                document_url: &worker.script_url,
                href: &href,
                method,
                body: request_body,
                content_type,
                request_headers: headers,
                credentials,
                credentials_mode: Some(credentials_mode),
                referrer_url,
                referrer_policy,
                cors_mode,
                redirect_mode,
                cache_mode,
                timeout: timeout_ms.map(|value| Duration::from_millis(u64::from(value))),
                max_response_bytes: None,
            })
            .await
    };
    let payload = match response {
        Ok(response) => service_worker_fetch_payload(response),
        Err(error) => json!({
            "error": error.to_string(),
            "timeout": false,
        }),
    };
    let resolved = worker.runtime.resolve_service_worker_fetch(
        worker.id,
        &worker.script_url,
        request_id,
        &payload,
        worker.is_module,
    )?;
    pending.extend(resolved.commands);
    *awaiting |= resolved.top_level_await_pending;
    if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
        *value = resolved_value;
        *awaiting = false;
    }
    Ok(true)
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
        | NativeScriptCommand::ServiceWorkerCachePutAll { request_id, .. }
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
        | NativeScriptCommand::ServiceWorkerCachePutAll { cache_name, .. }
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
        NativeScriptCommand::ServiceWorkerCachePutAll {
            cache_name,
            entries: batch_entries,
            ..
        } => {
            let mut staged_entries = caches.get(&cache_name).cloned().unwrap_or_default();
            for batch_entry in batch_entries {
                let NativeServiceWorkerCacheBatchEntry {
                    request_url,
                    request_method,
                    request_headers,
                    response,
                } = batch_entry;
                let request = validate_cache_request(&request_url, &request_method)?;
                if request.method != "GET" {
                    return Err(NativeEngineError::invalid(
                        "native service worker cache request method",
                        "Cache.addAll only supports GET requests",
                    ));
                }
                let entry = cache_entry_from_payload(&response, &request.url, request_headers)?;
                let key = cache_entry_key(&request.method, &request.url);
                if !staged_entries.contains_key(&key)
                    && staged_entries.len() >= MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES
                {
                    return Err(NativeEngineError::limit(
                        "native service worker cache entries",
                        MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
                        staged_entries.len().saturating_add(1),
                    ));
                }
                staged_entries.insert(key, entry);
            }
            caches.insert(cache_name, staged_entries);
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
    if response.status == 206 {
        return Err(NativeEngineError::invalid(
            "native service worker cached response status",
            "Cache.put and Cache.addAll must reject 206 Partial Content",
        ));
    }
    if response.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("vary") && value.split(',').any(|field| field.trim() == "*")
    }) {
        return Err(NativeEngineError::invalid(
            "native service worker cached response Vary header",
            "Cache.put and Cache.addAll must reject Vary: *",
        ));
    }
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
    _current_client_id: &str,
    fallback_url: &str,
    pending_open_windows: &mut VecDeque<NativeServiceWorkerOpenWindowRequest>,
) -> Result<NativeServiceWorkerFetchSettlement, NativeEngineError> {
    let mut pending = VecDeque::from(evaluation.commands);
    let mut value = evaluation.value;
    let mut awaiting = evaluation.top_level_await_pending;
    let mut client_messages = Vec::new();
    let mut open_window_request_id = None;
    let mut resolved_fetches = 0usize;
    if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
        value = resolved_value;
        awaiting = false;
    }
    while let Some(command) = pending.pop_front() {
        resolved_fetches = resolved_fetches.saturating_add(1);
        if resolved_fetches > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "service worker fetch event turns",
                MAX_NATIVE_MODULE_IMPORTS,
                resolved_fetches,
            ));
        }
        if apply_service_worker_lifecycle_command(worker, &command)? {
            continue;
        }
        if let Some(message) = service_worker_client_message_command(worker.id, command.clone())? {
            client_messages.push(message);
            continue;
        }
        if let Some(request) = service_worker_open_window_command(worker, command.clone())? {
            if pending_open_windows.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native service worker openWindow requests",
                    MAX_NATIVE_EFFECTS,
                    pending_open_windows.len().saturating_add(1),
                ));
            }
            open_window_request_id.get_or_insert(request.request_id);
            pending_open_windows.push_back(request);
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
        if !resolve_service_worker_fetch_command(
            worker,
            loader,
            command,
            &mut pending,
            &mut awaiting,
            &mut value,
        )
        .await?
        {
            return Err(NativeEngineError::Worker {
                operation: "service worker fetch event".into(),
                reason: "service worker emitted an unsupported host command".into(),
            });
        }
    }
    if awaiting {
        if let Some(resolved_value) = worker.runtime.take_top_level_await_result()? {
            return Ok(NativeServiceWorkerFetchSettlement::Complete {
                value: resolved_value,
                client_messages,
            });
        }
        if let Some(open_window_request_id) = open_window_request_id {
            return Ok(NativeServiceWorkerFetchSettlement::Suspended {
                continuation: NativeServiceWorkerFetchContinuation {
                    worker_id: worker.id,
                    open_window_request_id,
                    fallback_url: fallback_url.to_owned(),
                    pending,
                    value,
                    awaiting,
                },
                client_messages,
            });
        }
        return Err(NativeEngineError::Worker {
            operation: "service worker fetch event".into(),
            reason: "service worker fetch response promise remained pending".into(),
        });
    }
    Ok(NativeServiceWorkerFetchSettlement::Complete {
        value,
        client_messages,
    })
}

fn apply_service_worker_lifecycle_command(
    worker: &mut NativeServiceWorker,
    command: &NativeScriptCommand,
) -> Result<bool, NativeEngineError> {
    let worker_id = match command {
        NativeScriptCommand::ServiceWorkerSkipWaiting { worker_id }
        | NativeScriptCommand::ServiceWorkerClientsClaim { worker_id } => *worker_id,
        _ => return Ok(false),
    };
    if worker_id == 0 || worker_id != worker.id {
        return Err(NativeEngineError::Worker {
            operation: "service worker lifecycle command".into(),
            reason: "service worker lifecycle command owner is invalid".into(),
        });
    }
    if matches!(
        command,
        NativeScriptCommand::ServiceWorkerSkipWaiting { .. }
    ) {
        worker.skip_waiting_requested = true;
    } else {
        worker.clients_claim_requested = true;
    }
    Ok(true)
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
            | NativeScriptCommand::ServiceWorkerCachePutAll { .. }
            | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { .. }
            | NativeScriptCommand::ServiceWorkerCacheEntries { .. }
    )
}

fn service_worker_fetch_payload(response: NativeFetchResponse) -> Value {
    json!({
        "url": response.url,
        "status": response.status,
        "statusText": response.status_text,
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
        status_text: value
            .get("statusText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
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

pub(crate) fn native_service_worker_client_id(context_id: &str, frame_id: &str) -> String {
    // The browser backend owns context/frame identity. Hash both components
    // so the client id survives content-process replacement without exposing
    // a document URL as an opaque Service Worker client id.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in context_id
        .as_bytes()
        .iter()
        .chain(std::iter::once(&0xff))
        .chain(frame_id.as_bytes().iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("native-client-{hash:016x}")
}

fn native_service_worker_url_client_id(document_url: &str) -> String {
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

#[cfg(test)]
mod navigation_preload_tests {
    use super::*;

    fn test_worker(script_url: &str, scope: &str) -> NativeServiceWorker {
        NativeServiceWorker {
            id: 1,
            script_url: script_url.into(),
            scope: scope.into(),
            is_module: false,
            runtime: NativeJavaScriptRuntime::new_with_context_id("navigation-preload-test")
                .expect("test worker runtime constructs"),
            import_script_counts: BTreeMap::new(),
            skip_waiting_requested: false,
            clients_claim_requested: false,
            fetch_upload_connections: BTreeMap::new(),
        }
    }

    #[test]
    fn navigation_preload_state_requires_active_worker_and_survives_replacement() {
        let document_url = "https://navigation-preload.test/page";
        let scope = "https://navigation-preload.test/";
        let mut registry = NativeServiceWorkerRegistry::default();
        registry
            .registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: "https://navigation-preload.test/sw.js".into(),
                scope: scope.into(),
                worker_type: "classic".into(),
                navigation_preload_enabled: false,
                navigation_preload_header_value: "true".into(),
                waiting: None,
            });

        assert!(matches!(
            registry
                .apply_navigation_preload_operation(document_url, scope, "enable", None)
                .expect("inactive registration reports a Web API state failure"),
            NativeServiceWorkerNavigationPreloadResult::InvalidState
        ));
        assert!(matches!(
            registry
                .apply_navigation_preload_operation(document_url, scope, "getState", None)
                .expect("state can be read without an active worker"),
            NativeServiceWorkerNavigationPreloadResult::State {
                enabled: false,
                header_value,
            } if header_value == "true"
        ));

        registry.registrations.insert(
            scope.into(),
            test_worker("https://navigation-preload.test/sw.js", scope),
        );
        assert!(matches!(
            registry
                .apply_navigation_preload_operation(document_url, scope, "enable", None)
                .expect("active registration enables preloading"),
            NativeServiceWorkerNavigationPreloadResult::State {
                enabled: true,
                header_value,
            } if header_value == "true"
        ));
        registry
            .apply_navigation_preload_operation(
                document_url,
                scope,
                "setHeaderValue",
                Some("release-preview"),
            )
            .expect("active registration updates its header value");

        registry.remember_registration(&test_worker(
            "https://navigation-preload.test/sw-v2.js",
            scope,
        ));
        let profile = registry
            .registration_profiles()
            .into_iter()
            .find(|profile| profile.scope == scope)
            .expect("replacement keeps its registration profile");
        assert_eq!(
            profile.script_url,
            "https://navigation-preload.test/sw-v2.js"
        );
        assert!(profile.navigation_preload_enabled);
        assert_eq!(profile.navigation_preload_header_value, "release-preview");
        assert_eq!(
            registry.registration_changes().get(scope),
            Some(&Some(profile))
        );

        assert!(
            registry
                .apply_navigation_preload_operation(
                    "https://other.test/page",
                    scope,
                    "getState",
                    None,
                )
                .is_err()
        );
    }

    #[test]
    fn navigation_preload_requires_enabled_scope_and_fetch_listener() {
        let scope = "https://navigation-preload.test/";
        let target =
            Url::parse("https://navigation-preload.test/page").expect("navigation target parses");
        let mut registry = NativeServiceWorkerRegistry::default();
        registry
            .registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: "https://navigation-preload.test/sw.js".into(),
                scope: scope.into(),
                worker_type: "classic".into(),
                navigation_preload_enabled: false,
                navigation_preload_header_value: "true".into(),
                waiting: None,
            });
        let worker = test_worker("https://navigation-preload.test/sw.js", scope);
        assert!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("no active worker does not start a preload")
                .is_none()
        );
        registry.registrations.insert(scope.into(), worker);
        assert!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("an active worker without a fetch listener does not preload")
                .is_none()
        );
        let worker = registry
            .registrations
            .get_mut(scope)
            .expect("test registration retains its active worker");
        worker
            .runtime
            .evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                None,
                "self.addEventListener('fetch', () => {});",
                &BTreeMap::new(),
            )
            .expect("fetch listener installs");
        assert!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("disabled navigation preload does not start a request")
                .is_none()
        );
        registry
            .registration_profiles
            .iter_mut()
            .find(|profile| profile.scope == scope)
            .expect("test registration has a profile")
            .navigation_preload_enabled = true;
        assert!(
            registry
                .navigation_preload_configuration(&target, "POST")
                .expect("non-GET navigation does not start a preload")
                .is_none()
        );
        assert_eq!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("enabled registration with a fetch listener starts a preload"),
            Some((scope.into(), "true".into()))
        );
        assert!(
            registry
                .navigation_preload_configuration(
                    &Url::parse("https://other.test/page").expect("cross-origin URL parses"),
                    "GET",
                )
                .expect("cross-origin navigation cannot match this registration")
                .is_none()
        );
    }
}
