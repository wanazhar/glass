use super::*;

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

    pub(super) fn current_client_id_for(&self, document_url: &str) -> String {
        let canonical = without_fragment(document_url);
        if self.current_client_url.as_deref() == Some(canonical)
            && !self.current_client_id.is_empty()
        {
            return self.current_client_id.clone();
        }
        native_service_worker_url_client_id(canonical)
    }

    pub(super) fn current_client_is_controlled(&self, document_url: &str) -> bool {
        without_fragment(document_url) == self.current_client_url.as_deref().unwrap_or_default()
            && self.current_client_scope.is_some()
    }

    pub(super) fn current_client_scope_for(&self, document_url: &str) -> Option<&str> {
        self.current_client_is_controlled(document_url)
            .then_some(self.current_client_scope.as_deref())
            .flatten()
    }

    pub(super) fn claim_current_client(&mut self, scope: &str) -> Result<(), NativeEngineError> {
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

    pub(super) fn ensure_current_client_state(&mut self) -> Result<(), NativeEngineError> {
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

    pub(super) fn update_current_client_state_url(&mut self, document_url: &str) {
        let canonical = without_fragment(document_url);
        if let Some(state) = self
            .client_states
            .iter_mut()
            .find(|state| state.id == self.current_client_id)
        {
            state.url = canonical.to_owned();
        }
    }

    pub(super) fn client_states_for_worker(
        &self,
        scope: &str,
    ) -> Result<Vec<Value>, NativeEngineError> {
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

    pub(super) fn set_worker_client_view(
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
        request_id: u32,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
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
                        NativeServiceWorkerRestoreRequest {
                            owner_url: document_url,
                            script_url: &profile.script_url,
                            scope: &profile.scope,
                            worker_type: &profile.worker_type,
                            request_id,
                        },
                        parent_fetch_broker.as_deref_mut(),
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
                    NativeServiceWorkerRestoreRequest {
                        owner_url: document_url,
                        script_url: &waiting.script_url,
                        scope: &profile.scope,
                        worker_type: &waiting.worker_type,
                        request_id,
                    },
                    parent_fetch_broker.as_deref_mut(),
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

    pub(super) fn next_worker_id(&mut self) -> Result<u32, NativeEngineError> {
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

    pub(super) async fn instantiate_worker(
        &mut self,
        loader: &mut NativeResourceLoader,
        request: NativeServiceWorkerInstantiation,
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeServiceWorker, NativeEngineError> {
        let NativeServiceWorkerInstantiation {
            script_url,
            scope,
            is_module,
            source,
            import_script_counts,
            module_graph,
            worker_referrer_policy,
        } = request;
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
        let client_messages = settle_service_worker_cache_event_with_parent_fetch_broker(
            &mut worker,
            initial,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
            NativeServiceWorkerFetchAuthority {
                parent_fetch_broker,
                allow_captured_load_fetch: true,
            },
        )
        .await?;
        self.enqueue_client_messages(client_messages)?;
        Ok(worker)
    }

    pub(super) async fn restore_worker(
        &mut self,
        loader: &mut NativeResourceLoader,
        request: NativeServiceWorkerRestoreRequest<'_>,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeServiceWorker, NativeEngineError> {
        let NativeServiceWorkerRestoreRequest {
            owner_url,
            script_url,
            scope,
            worker_type,
            request_id,
        } = request;
        let resource = if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
            load_service_worker_entry(
                loader,
                owner_url,
                script_url,
                script_url,
                request_id,
                None,
                Some(parent_fetch_broker),
            )
            .await?
        } else if is_parent_owned_network_target(owner_url, script_url) {
            return Err(missing_parent_network_authority(
                "Service Worker restoration",
            ));
        } else {
            loader
                .load_worker_async(script_url, script_url, MAX_NATIVE_SCRIPT_BYTES)
                .await?
        }
        .ok_or_else(|| NativeEngineError::Network {
            operation: "service worker restoration".into(),
            reason: "persisted service worker script was blocked or unavailable".into(),
        })?;
        let is_module = worker_type.eq_ignore_ascii_case("module");
        let root_request_url = resolve_module_request_url(script_url, script_url)?;
        let source_owner_url = if parent_fetch_broker.is_some() {
            owner_url
        } else {
            script_url
        };
        let (source, import_script_counts, module_graph, worker_referrer_policy) =
            load_service_worker_source(
                loader,
                source_owner_url,
                root_request_url,
                resource.clone(),
                is_module,
                request_id,
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
        self.instantiate_worker(
            loader,
            NativeServiceWorkerInstantiation {
                script_url: resource.url,
                scope: scope.to_owned(),
                is_module,
                source,
                import_script_counts,
                module_graph,
                worker_referrer_policy,
            },
            parent_fetch_broker,
        )
        .await
    }

    pub(super) async fn settle_worker_install(
        &mut self,
        worker: &mut NativeServiceWorker,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        self.set_worker_client_view(worker)?;
        let install = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "install",
            worker.is_module,
        )?;
        let client_messages = settle_service_worker_cache_event_with_parent_fetch_broker(
            worker,
            install,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
            NativeServiceWorkerFetchAuthority {
                parent_fetch_broker,
                allow_captured_load_fetch: true,
            },
        )
        .await?;
        self.enqueue_client_messages(client_messages)
    }

    pub(super) async fn settle_worker_activate(
        &mut self,
        worker: &mut NativeServiceWorker,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        self.set_worker_client_view(worker)?;
        let activate = worker.runtime.evaluate_service_worker_lifecycle(
            worker.id,
            &worker.script_url,
            "activate",
            worker.is_module,
        )?;
        let client_messages = settle_service_worker_cache_event_with_parent_fetch_broker(
            worker,
            activate,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
            NativeServiceWorkerFetchAuthority {
                parent_fetch_broker,
                allow_captured_load_fetch: true,
            },
        )
        .await?;
        self.enqueue_client_messages(client_messages)
    }

    pub(super) fn commit_activated_worker(
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

    pub(super) async fn install_worker(
        &mut self,
        mut worker: NativeServiceWorker,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let scope = worker.scope.clone();
        self.settle_worker_install(&mut worker, loader, parent_fetch_broker.as_deref_mut())
            .await?;
        if worker.skip_waiting_requested || !self.registrations.contains_key(&scope) {
            self.settle_worker_activate(&mut worker, loader, parent_fetch_broker)
                .await?;
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
        request: NativeServiceWorkerRegistrationRequest<'_>,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let NativeServiceWorkerRegistrationRequest {
            owner_url,
            script_url: _,
            scope,
            worker_type: _,
            referrer_url,
            referrer_policy,
            request_id,
        } = request;
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
        let resource = load_service_worker_entry(
            loader,
            owner_url,
            referrer_url,
            &profile.script_url,
            request_id,
            Some(referrer_policy),
            parent_fetch_broker.as_deref_mut(),
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
                request_id,
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
        let worker = self
            .instantiate_worker(
                loader,
                NativeServiceWorkerInstantiation {
                    script_url: resource.url,
                    scope: profile.scope.clone(),
                    is_module,
                    source,
                    import_script_counts,
                    module_graph,
                    worker_referrer_policy,
                },
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
        self.install_worker(worker, loader, parent_fetch_broker)
            .await
    }

    pub(super) fn remember_registration(&mut self, worker: &NativeServiceWorker) {
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

    pub(super) fn remember_waiting_registration(
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

    pub(super) fn record_registration_change(&mut self, scope: &str) {
        let profile = self
            .registration_profiles
            .iter()
            .find(|profile| profile.scope == scope)
            .cloned();
        self.registration_changes.insert(scope.to_owned(), profile);
    }

    pub(crate) async fn register(
        &mut self,
        request: NativeServiceWorkerRegistrationRequest<'_>,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
        let NativeServiceWorkerRegistrationRequest {
            owner_url,
            script_url: Some(script_url),
            scope,
            worker_type: Some(worker_type),
            referrer_url,
            referrer_policy,
            request_id,
        } = request
        else {
            return Err(NativeEngineError::invalid(
                "service worker registration request",
                "script URL and worker type are required",
            ));
        };
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
        let resource = load_service_worker_entry(
            loader,
            owner_url,
            referrer_url,
            &script,
            request_id,
            Some(referrer_policy),
            parent_fetch_broker.as_deref_mut(),
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
                request_id,
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
        let worker = self
            .instantiate_worker(
                loader,
                NativeServiceWorkerInstantiation {
                    script_url: resource.url,
                    scope: scope.clone(),
                    is_module,
                    source,
                    import_script_counts,
                    module_graph,
                    worker_referrer_policy,
                },
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
        self.install_worker(worker, loader, parent_fetch_broker)
            .await
    }
}
