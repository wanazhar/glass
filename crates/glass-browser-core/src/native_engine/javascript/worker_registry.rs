use super::*;

mod message_ports;
mod network_workers;

struct NativeWorkerFetchUploadRequest {
    worker_id: u32,
    request_id: u32,
    worker_url: String,
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
}

struct NativeWorkerCreateRequest {
    worker_id: u32,
    href: String,
    worker_type: String,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
}

struct NativeSharedWorkerCreateRequest {
    connection_id: u32,
    href: String,
    name: String,
    worker_type: String,
    credentials: String,
    extended_lifetime: bool,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
    constructor_storage_key: NativeSharedWorkerStorageKey,
    transfer_port: NativeMessagePortTransfer,
}

struct NativeDynamicWorkerModuleRequest<'a> {
    worker_id: u32,
    request_id: u32,
    worker_url: &'a str,
    module_referrer: &'a str,
    specifier: &'a str,
    module_type: Option<NativeModuleResourceType>,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
}

struct NativeWorkerClassicGraphOptions {
    dynamic_import_mode: NativeWorkerClassicDynamicImportMode,
    worker_referrer_policy: NativeFetchReferrerPolicy,
    request_id: u32,
    parent_loader_owns_network: bool,
}

struct NativeWorkerModuleGraphOptions<'a> {
    owner_url: &'a str,
    root_request_url: String,
    credentials_mode: Option<&'a str>,
    request_id: u32,
    inherited_referrer_policy: Option<NativeFetchReferrerPolicy>,
    parent_loader_owns_network: bool,
}

impl NativeWorkerRegistry {
    pub(crate) fn new() -> Self {
        Self::new_inner(false, false)
    }

    /// Construct a registry with worker response streams enabled. The
    /// content-process owner uses this for HTTP(S) responses and the inline
    /// owner uses it for registered fixture responses; both feed the same
    /// demand-driven transport and serialized worker event pump.
    pub(crate) fn new_with_fetch_streams() -> Self {
        Self::new_inner(true, false)
    }

    /// Construct the browser-parent SharedWorker registry. Only this owner
    /// may use its parent-side loader for network resources when no child IPC
    /// broker exists; sandboxed content registries remain broker-only.
    pub fn new_with_parent_network_authority() -> Self {
        Self::new_inner(true, true)
    }

    pub(super) fn new_inner(stream_worker_fetches: bool, parent_loader_owns_network: bool) -> Self {
        Self {
            workers: BTreeMap::new(),
            shared_worker_keys: BTreeMap::new(),
            shared_worker_owners: BTreeMap::new(),
            next_worker_timer_id: 0,
            pending_messages: VecDeque::new(),
            pending_message_port_messages: VecDeque::new(),
            pending_page_message_port_commands: VecDeque::new(),
            pending_websocket_commands: VecDeque::new(),
            pending_event_source_commands: VecDeque::new(),
            worker_fetch_stream_connections: BTreeMap::new(),
            worker_fetch_upload_connections: BTreeMap::new(),
            stream_worker_fetches,
            parent_loader_owns_network,
            message_port_routes: BTreeMap::new(),
        }
    }

    pub(super) fn evaluate_worker_with_loader<F>(
        &mut self,
        worker_id: u32,
        loader: &mut NativeResourceLoader,
        evaluate: F,
    ) -> Result<NativeScriptEvaluation, NativeEngineError>
    where
        F: FnOnce(&NativeDedicatedWorker) -> Result<NativeScriptEvaluation, NativeEngineError>,
    {
        let worker = self.workers.get(&worker_id).ok_or_else(|| {
            NativeEngineError::invalid("native Worker", "worker no longer exists")
        })?;
        worker.runtime.set_sync_xhr_loader(loader);
        let result = evaluate(worker);
        if let Some(updated_loader) = worker.runtime.take_sync_xhr_loader() {
            loader.merge_fetch_task_state(updated_loader)?;
        }
        result
    }

    pub fn clear(&mut self) {
        self.workers.clear();
        self.shared_worker_keys.clear();
        self.shared_worker_owners.clear();
        self.next_worker_timer_id = 0;
        self.pending_messages.clear();
        self.pending_message_port_messages.clear();
        self.pending_page_message_port_commands.clear();
        self.pending_websocket_commands.clear();
        self.pending_event_source_commands.clear();
        for (_, connection) in std::mem::take(&mut self.worker_fetch_stream_connections) {
            let _ = connection
                .commands
                .try_send(NativeFetchStreamCommand::Cancel);
        }
        for (_, connection) in std::mem::take(&mut self.worker_fetch_upload_connections) {
            let _ = connection
                .commands
                .try_send(NativeFetchUploadCommand::Cancel);
        }
        self.message_port_routes.clear();
    }

    pub fn shared_worker_id_for_port(&self, bridge_key: &str) -> Option<u32> {
        let worker_id = self.message_port_routes.get(bridge_key)?.worker_id;
        self.workers
            .get(&worker_id)
            .is_some_and(|worker| worker.is_shared)
            .then_some(worker_id)
    }

    pub fn add_shared_worker_owner(
        &mut self,
        worker_id: u32,
        context_id: String,
        frame_id: String,
        document_generation: u32,
    ) -> Result<(), NativeEngineError> {
        for (field, value) in [
            ("SharedWorker owner context", context_id.as_str()),
            ("SharedWorker owner frame", frame_id.as_str()),
        ] {
            if value.is_empty() {
                return Err(NativeEngineError::invalid(field, "must not be empty"));
            }
            if value.len() > crate::limits::MAX_BACKEND_ID_BYTES {
                return Err(NativeEngineError::limit(
                    field,
                    crate::limits::MAX_BACKEND_ID_BYTES,
                    value.len(),
                ));
            }
            if value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
                return Err(NativeEngineError::invalid(
                    field,
                    "must not contain control characters",
                ));
            }
        }
        if document_generation == 0 {
            return Err(NativeEngineError::invalid(
                "SharedWorker owner Document generation",
                "must be positive",
            ));
        }
        if !self
            .workers
            .get(&worker_id)
            .is_some_and(|worker| worker.is_shared)
        {
            return Err(NativeEngineError::invalid(
                "SharedWorker owner",
                "owner can only be registered for a live shared worker",
            ));
        }
        let owners = self.shared_worker_owners.entry(worker_id).or_default();
        let owner = (context_id, frame_id, document_generation);
        if !owners.contains(&owner) && owners.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native SharedWorker Document owners",
                MAX_NATIVE_WORKER_MESSAGES,
                owners.len().saturating_add(1),
            ));
        }
        owners.insert(owner);
        Ok(())
    }

    pub fn remove_shared_worker_owners_for_context(&mut self, context_id: &str) -> BTreeSet<u32> {
        self.remove_shared_worker_owners_matching(|owner_context, _, _| owner_context == context_id)
    }

    pub fn remove_shared_worker_owner_for_document(
        &mut self,
        context_id: &str,
        frame_id: &str,
        document_generation: u32,
    ) -> BTreeSet<u32> {
        self.remove_shared_worker_owners_matching(|owner_context, owner_frame, owner_generation| {
            owner_context == context_id
                && owner_frame == frame_id
                && owner_generation == document_generation
        })
    }

    pub fn remove_shared_worker_owners_for_frame(
        &mut self,
        context_id: &str,
        frame_id: &str,
    ) -> BTreeSet<u32> {
        self.remove_shared_worker_owners_matching(|owner_context, owner_frame, _| {
            owner_context == context_id && owner_frame == frame_id
        })
    }

    pub(super) fn remove_shared_worker_owners_matching(
        &mut self,
        mut should_remove: impl FnMut(&str, &str, u32) -> bool,
    ) -> BTreeSet<u32> {
        let worker_ids = self
            .shared_worker_owners
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut unowned = BTreeSet::new();
        for worker_id in worker_ids {
            let Some(owners) = self.shared_worker_owners.get_mut(&worker_id) else {
                continue;
            };
            owners.retain(|(owner_context, owner_frame, owner_generation)| {
                !should_remove(owner_context, owner_frame, *owner_generation)
            });
            if owners.is_empty() {
                self.shared_worker_owners.remove(&worker_id);
                unowned.insert(worker_id);
            }
        }
        unowned
    }

    pub fn terminate_unowned_shared_worker(&mut self, worker_id: u32) -> bool {
        if self
            .shared_worker_owners
            .get(&worker_id)
            .is_some_and(|owners| !owners.is_empty())
            || !self
                .workers
                .get(&worker_id)
                .is_some_and(|worker| worker.is_shared)
        {
            return false;
        }
        self.workers.remove(&worker_id);
        self.remove_worker_routes(worker_id);
        true
    }

    pub(crate) fn take_messages(&mut self) -> Vec<NativeWorkerMessage> {
        self.pending_messages.drain(..).collect()
    }

    pub fn take_messages_for_worker(&mut self, worker_id: u32) -> Vec<NativeWorkerMessage> {
        let mut messages = Vec::new();
        self.pending_messages.retain(|message| {
            if message.worker_id == worker_id {
                messages.push(message.clone());
                false
            } else {
                true
            }
        });
        messages
    }

    pub fn take_message_port_messages(&mut self) -> Vec<NativeMessagePortPageMessage> {
        self.pending_message_port_messages.drain(..).collect()
    }

    pub(crate) fn take_page_message_port_commands(&mut self) -> Vec<NativePageMessagePortCommand> {
        self.pending_page_message_port_commands.drain(..).collect()
    }

    pub fn take_websocket_commands(&mut self) -> Vec<NativeWorkerWebSocketCommand> {
        self.pending_websocket_commands.drain(..).collect()
    }

    pub fn take_event_source_commands(&mut self) -> Vec<NativeWorkerEventSourceCommand> {
        self.pending_event_source_commands.drain(..).collect()
    }

    pub(crate) fn next_timer_delay_ms(&self) -> Result<Option<u64>, NativeEngineError> {
        let mut earliest = None;
        for worker in self.workers.values() {
            if let Some(delay) = worker.runtime.next_worker_timer_delay_ms()? {
                earliest = Some(earliest.map_or(delay, |current: u64| current.min(delay)));
            }
        }
        Ok(earliest)
    }

    /// Run one due timer turn for the next worker that has work ready. Worker
    /// callbacks stay inside their isolated realm and can only emit the
    /// bounded worker message/lifecycle/fetch commands. A rotating cursor
    /// prevents the lowest worker id from monopolizing each page boundary.
    pub(crate) async fn run_due_timers(
        &mut self,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.run_due_timers_inner(loader, None).await
    }

    pub(crate) async fn run_due_timers_with_parent_fetch_broker(
        &mut self,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<(), NativeEngineError> {
        self.run_due_timers_inner(loader, Some(parent_fetch_broker))
            .await
    }

    async fn run_due_timers_inner<'broker>(
        &mut self,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<(), NativeEngineError> {
        let worker_ids = self.workers.keys().copied().collect::<Vec<_>>();
        let mut selected_worker_id = None;
        let after_cursor = worker_ids
            .iter()
            .copied()
            .filter(|worker_id| *worker_id > self.next_worker_timer_id);
        let before_cursor = worker_ids
            .iter()
            .copied()
            .filter(|worker_id| *worker_id <= self.next_worker_timer_id);
        for worker_id in after_cursor.chain(before_cursor) {
            let Some(worker) = self.workers.get(&worker_id) else {
                continue;
            };
            if worker
                .runtime
                .next_worker_timer_delay_ms()?
                .is_some_and(|delay| delay == 0)
            {
                selected_worker_id = Some(worker_id);
                break;
            }
        }
        let Some(worker_id) = selected_worker_id else {
            return Ok(());
        };
        self.next_worker_timer_id = worker_id;
        let evaluation = self.evaluate_worker_with_loader(worker_id, loader, |worker| {
            worker.evaluate_turn(
                worker_id,
                "globalThis.__glassRunWorkerTimers(performance.now());",
            )
        });
        match evaluation {
            Ok(evaluation) => {
                self.collect_worker_evaluation_with_parent_fetch_broker(
                    worker_id,
                    evaluation,
                    loader,
                    parent_fetch_broker,
                )
                .await?
            }
            Err(error) => {
                let worker_url = self
                    .workers
                    .get(&worker_id)
                    .map(|worker| worker.url.clone())
                    .unwrap_or_default();
                self.workers.remove(&worker_id);
                self.remove_worker_routes(worker_id);
                self.queue_error(worker_id, &worker_url, &error.to_string())?;
            }
        }
        Ok(())
    }

    pub async fn apply_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        owner_url: &str,
    ) -> Result<(), NativeEngineError> {
        self.apply_commands_inner(commands, loader, owner_url, None)
            .await
    }

    pub(crate) async fn apply_commands_with_parent_fetch_broker(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        owner_url: &str,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<(), NativeEngineError> {
        self.apply_commands_inner(commands, loader, owner_url, Some(parent_fetch_broker))
            .await
    }

    async fn apply_commands_inner<'broker>(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        owner_url: &str,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<(), NativeEngineError> {
        if commands.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Worker commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len(),
            ));
        }
        for command in commands {
            match command {
                NativeScriptCommand::SharedWorkerCreate {
                    connection_id,
                    href,
                    name,
                    worker_type,
                    credentials,
                    extended_lifetime,
                    referrer_policy,
                    constructor_storage_key,
                    transfer_port,
                } => {
                    let constructor_storage_key = constructor_storage_key.ok_or_else(|| {
                        NativeEngineError::invalid(
                            "native SharedWorker constructor storage key",
                            "must be supplied by the browser owner",
                        )
                    })?;
                    let referrer_policy = referrer_policy
                        .as_deref()
                        .map(NativeFetchReferrerPolicy::parse)
                        .transpose()?;
                    self.create_shared_worker(
                        NativeSharedWorkerCreateRequest {
                            connection_id,
                            href,
                            name,
                            worker_type,
                            credentials,
                            extended_lifetime,
                            referrer_policy,
                            constructor_storage_key,
                            transfer_port,
                        },
                        loader,
                        owner_url,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                }
                NativeScriptCommand::WorkerCreate {
                    worker_id,
                    href,
                    worker_type,
                    referrer_policy,
                } => {
                    let referrer_policy = referrer_policy
                        .as_deref()
                        .map(NativeFetchReferrerPolicy::parse)
                        .transpose()?;
                    self.create_worker(
                        NativeWorkerCreateRequest {
                            worker_id,
                            href,
                            worker_type,
                            referrer_policy,
                        },
                        loader,
                        owner_url,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                }
                NativeScriptCommand::WorkerPostMessage {
                    worker_id,
                    data,
                    transfer_ports,
                    object_urls,
                } => {
                    self.post_message(
                        worker_id,
                        data,
                        transfer_ports,
                        object_urls,
                        loader,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                }
                NativeScriptCommand::WorkerTerminate { worker_id }
                | NativeScriptCommand::WorkerClose { worker_id } => {
                    self.workers.remove(&worker_id);
                    self.remove_worker_routes(worker_id);
                }
                _ => {
                    return Err(NativeEngineError::invalid(
                        "native Worker command",
                        "command was not routed through the Worker host boundary",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn create_worker(
        &mut self,
        request: NativeWorkerCreateRequest,
        loader: &mut NativeResourceLoader,
        owner_url: &str,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        let NativeWorkerCreateRequest {
            worker_id,
            href,
            worker_type,
            referrer_policy,
        } = request;
        if worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "native Worker id",
                "must be positive",
            ));
        }
        if self.workers.len() >= MAX_NATIVE_WORKERS {
            return Err(NativeEngineError::limit(
                "native Workers",
                MAX_NATIVE_WORKERS,
                self.workers.len().saturating_add(1),
            ));
        }
        if self.workers.contains_key(&worker_id) {
            return Err(NativeEngineError::invalid(
                "native Worker id",
                "must be unique within the page realm",
            ));
        }
        validate_url_text("native Worker URL", &href)?;
        let is_module = worker_type.eq_ignore_ascii_case("module");
        if !worker_type.is_empty() && !worker_type.eq_ignore_ascii_case("classic") && !is_module {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native Worker type must be classic or module".into(),
            });
        }
        let module_request_url = if is_module {
            match resolve_module_request_url(owner_url, &href) {
                Ok(url) => url,
                Err(error) => {
                    self.queue_error(worker_id, &href, &error.to_string())?;
                    return Ok(());
                }
            }
        } else {
            String::new()
        };

        let resource_result = load_worker_script_resource(
            loader,
            parent_fetch_broker.as_deref_mut(),
            NativeWorkerScriptResourceRequest {
                request_id: worker_id,
                document_url: owner_url,
                href: &href,
                referrer_url: None,
                max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                module_type: None,
                credentials_mode: None,
                referrer_policy,
                shared_worker_module_entry: false,
                parent_loader_owns_network: self.parent_loader_owns_network,
            },
        )
        .await;
        let resource = match resource_result {
            Ok(Some(resource)) => resource,
            Ok(None) => {
                self.queue_error(worker_id, &href, "worker script was blocked or unavailable")?;
                return Ok(());
            }
            Err(error) => {
                self.queue_error(worker_id, &href, &error.to_string())?;
                return Ok(());
            }
        };
        let worker_referrer_policy = effective_worker_global_referrer_policy(
            &resource.url,
            referrer_policy,
            resource.response_referrer_policy,
        );
        let (
            source,
            import_script_counts,
            module_name,
            module_base_url,
            module_sources,
            module_base_urls,
            module_referrer_policies,
            dynamic_import_referrers,
        ) = if is_module {
            let graph = match self
                .load_worker_module_graph(
                    loader,
                    NativeWorkerModuleGraphOptions {
                        owner_url,
                        root_request_url: module_request_url,
                        credentials_mode: None,
                        request_id: worker_id,
                        inherited_referrer_policy: Some(
                            referrer_policy
                                .unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin),
                        ),
                        parent_loader_owns_network: self.parent_loader_owns_network,
                    },
                    resource.clone(),
                    parent_fetch_broker.as_deref_mut(),
                )
                .await
            {
                Ok(graph) => graph,
                Err(error) => {
                    self.queue_error(worker_id, &resource.url, &error.to_string())?;
                    return Ok(());
                }
            };
            (
                resource.body.clone(),
                BTreeMap::new(),
                graph.root_name,
                graph.root_base_url,
                graph.sources,
                graph.base_urls,
                graph.referrer_policies,
                BTreeSet::new(),
            )
        } else {
            let graph = match self
                .load_worker_script_graph(
                    loader,
                    resource.clone(),
                    NativeWorkerClassicGraphOptions {
                        dynamic_import_mode: NativeWorkerClassicDynamicImportMode::RewritePerScript,
                        worker_referrer_policy,
                        request_id: worker_id,
                        parent_loader_owns_network: self.parent_loader_owns_network,
                    },
                    parent_fetch_broker.as_deref_mut(),
                )
                .await
            {
                Ok(graph) => graph,
                Err(error) => {
                    self.queue_error(worker_id, &resource.url, &error.to_string())?;
                    return Ok(());
                }
            };
            (
                graph.source,
                graph.import_script_counts,
                resource.url.clone(),
                resource.url.clone(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                graph.dynamic_import_referrers,
            )
        };
        let mut runtime =
            match NativeJavaScriptRuntime::new_with_context_id(format!("glass-worker-{worker_id}"))
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    self.queue_error(worker_id, &resource.url, &error.to_string())?;
                    return Ok(());
                }
            };
        runtime.worker_global_referrer_policy = Some(worker_referrer_policy);
        self.workers.insert(
            worker_id,
            NativeDedicatedWorker {
                url: resource.url.clone(),
                module_name,
                module_base_url,
                import_script_counts,
                top_level_await_pending: false,
                dynamic_import_referrers,
                module_sources,
                module_base_urls,
                module_referrer_policies,
                is_module,
                is_shared: false,
                credentials: "same-origin".into(),
                extended_lifetime: false,
                next_module_turn: AtomicU64::new(1),
                runtime,
            },
        );
        let evaluation = {
            self.evaluate_worker_with_loader(worker_id, loader, |worker| {
                worker.evaluate_initial(worker_id, &source)
            })
        };
        match evaluation {
            Ok(evaluation) => {
                self.collect_worker_evaluation_with_parent_fetch_broker(
                    worker_id,
                    evaluation,
                    loader,
                    parent_fetch_broker,
                )
                .await
            }
            Err(error) => {
                self.workers.remove(&worker_id);
                self.remove_worker_routes(worker_id);
                self.queue_error(worker_id, &resource.url, &error.to_string())
            }
        }
    }

    async fn create_shared_worker(
        &mut self,
        request: NativeSharedWorkerCreateRequest,
        loader: &mut NativeResourceLoader,
        owner_url: &str,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        let NativeSharedWorkerCreateRequest {
            connection_id,
            href,
            name,
            worker_type,
            credentials,
            extended_lifetime,
            referrer_policy,
            constructor_storage_key,
            transfer_port,
        } = request;
        if connection_id == 0 {
            return Err(NativeEngineError::invalid(
                "native SharedWorker connection id",
                "must be positive",
            ));
        }
        validate_url_text("native SharedWorker URL", &href)?;
        if name.len() > MAX_NATIVE_SHARED_WORKER_NAME_BYTES {
            return Err(NativeEngineError::limit(
                "native SharedWorker name",
                MAX_NATIVE_SHARED_WORKER_NAME_BYTES,
                name.len(),
            ));
        }
        if name.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
            return Err(NativeEngineError::invalid(
                "native SharedWorker name",
                "must not contain control characters",
            ));
        }
        let is_module = validate_native_shared_worker_options(&worker_type, &credentials)?;
        let module_request_url = if is_module {
            match resolve_module_request_url(owner_url, &href) {
                Ok(url) => url,
                Err(error) => {
                    self.queue_error(connection_id, &href, &error.to_string())?;
                    return Ok(());
                }
            }
        } else {
            String::new()
        };
        validate_message_port_transfers(std::slice::from_ref(&transfer_port))?;
        let shared_key = NativeSharedWorkerKey {
            constructor_storage_key,
            constructor_url: href.clone(),
            name: name.clone(),
        };
        if let Some(worker_id) = self.shared_worker_keys.get(&shared_key).copied() {
            if let Some(worker) = self.workers.get(&worker_id) {
                if worker.is_module != is_module
                    || worker.credentials != credentials
                    || worker.extended_lifetime != extended_lifetime
                {
                    self.queue_error(
                        connection_id,
                        &href,
                        "SharedWorker options do not match the existing worker",
                    )?;
                    return Ok(());
                }
                let worker_url = worker.url.clone();
                self.register_page_transfers(worker_id, std::slice::from_ref(&transfer_port))?;
                let evaluation = self.evaluate_worker_with_loader(worker_id, loader, |worker| {
                    worker.evaluate_shared_connect(worker_id, &transfer_port)
                });
                match evaluation {
                    Ok(evaluation) => {
                        if let Err(error) = self
                            .collect_worker_evaluation_with_parent_fetch_broker(
                                worker_id,
                                evaluation,
                                loader,
                                parent_fetch_broker.as_deref_mut(),
                            )
                            .await
                        {
                            self.remove_transfer_routes(std::slice::from_ref(&transfer_port));
                            return Err(error);
                        }
                    }
                    Err(error) => {
                        self.remove_transfer_routes(std::slice::from_ref(&transfer_port));
                        self.workers.remove(&worker_id);
                        self.remove_worker_routes(worker_id);
                        self.queue_error(connection_id, &worker_url, &error.to_string())?;
                    }
                }
                return Ok(());
            }
            self.shared_worker_keys.remove(&shared_key);
        }
        if self.workers.len() >= MAX_NATIVE_WORKERS {
            return Err(NativeEngineError::limit(
                "native workers",
                MAX_NATIVE_WORKERS,
                self.workers.len().saturating_add(1),
            ));
        }
        if self.workers.contains_key(&connection_id) {
            return Err(NativeEngineError::invalid(
                "native SharedWorker connection id",
                "must be unique within the page realm",
            ));
        }
        let resource_result = load_worker_script_resource(
            loader,
            parent_fetch_broker.as_deref_mut(),
            NativeWorkerScriptResourceRequest {
                request_id: connection_id,
                document_url: owner_url,
                href: &href,
                referrer_url: None,
                max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                module_type: None,
                credentials_mode: is_module.then_some(credentials.as_str()),
                referrer_policy,
                shared_worker_module_entry: is_module,
                parent_loader_owns_network: self.parent_loader_owns_network,
            },
        )
        .await;
        let resource = match resource_result {
            Ok(Some(resource)) => resource,
            Ok(None) => {
                self.queue_error(
                    connection_id,
                    &href,
                    "SharedWorker script was blocked or unavailable",
                )?;
                return Ok(());
            }
            Err(error) => {
                self.queue_error(connection_id, &href, &error.to_string())?;
                return Ok(());
            }
        };
        let worker_referrer_policy = effective_worker_global_referrer_policy(
            &resource.url,
            referrer_policy,
            resource.response_referrer_policy,
        );
        let (
            source,
            import_script_counts,
            module_name,
            module_base_url,
            module_sources,
            module_base_urls,
            module_referrer_policies,
            dynamic_import_referrers,
        ) = if is_module {
            let graph = match self
                .load_worker_module_graph(
                    loader,
                    NativeWorkerModuleGraphOptions {
                        owner_url,
                        root_request_url: module_request_url,
                        credentials_mode: Some(&credentials),
                        request_id: connection_id,
                        inherited_referrer_policy: Some(
                            referrer_policy
                                .unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin),
                        ),
                        parent_loader_owns_network: self.parent_loader_owns_network,
                    },
                    resource.clone(),
                    parent_fetch_broker.as_deref_mut(),
                )
                .await
            {
                Ok(graph) => graph,
                Err(error) => {
                    self.queue_error(connection_id, &resource.url, &error.to_string())?;
                    return Ok(());
                }
            };
            (
                resource.body.clone(),
                BTreeMap::new(),
                graph.root_name,
                graph.root_base_url,
                graph.sources,
                graph.base_urls,
                graph.referrer_policies,
                BTreeSet::new(),
            )
        } else {
            let graph = match self
                .load_worker_script_graph(
                    loader,
                    resource.clone(),
                    NativeWorkerClassicGraphOptions {
                        dynamic_import_mode: NativeWorkerClassicDynamicImportMode::RewritePerScript,
                        worker_referrer_policy,
                        request_id: connection_id,
                        parent_loader_owns_network: self.parent_loader_owns_network,
                    },
                    parent_fetch_broker.as_deref_mut(),
                )
                .await
            {
                Ok(graph) => graph,
                Err(error) => {
                    self.queue_error(connection_id, &resource.url, &error.to_string())?;
                    return Ok(());
                }
            };
            (
                graph.source,
                graph.import_script_counts,
                resource.url.clone(),
                resource.url.clone(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                graph.dynamic_import_referrers,
            )
        };
        let mut runtime = match NativeJavaScriptRuntime::new_with_context_id(format!(
            "glass-shared-worker-{connection_id}"
        )) {
            Ok(runtime) => runtime,
            Err(error) => {
                self.queue_error(connection_id, &resource.url, &error.to_string())?;
                return Ok(());
            }
        };
        runtime.worker_global_referrer_policy = Some(worker_referrer_policy);
        self.workers.insert(
            connection_id,
            NativeDedicatedWorker {
                url: resource.url.clone(),
                module_name,
                module_base_url,
                import_script_counts,
                top_level_await_pending: false,
                dynamic_import_referrers,
                module_sources,
                module_base_urls,
                module_referrer_policies,
                is_module,
                is_shared: true,
                credentials,
                extended_lifetime,
                next_module_turn: AtomicU64::new(1),
                runtime,
            },
        );
        self.shared_worker_keys
            .insert(shared_key.clone(), connection_id);
        if let Err(error) =
            self.register_page_transfers(connection_id, std::slice::from_ref(&transfer_port))
        {
            self.workers.remove(&connection_id);
            self.shared_worker_keys.remove(&shared_key);
            return Err(error);
        }
        let initial = self.evaluate_worker_with_loader(connection_id, loader, |worker| {
            worker.evaluate_initial(connection_id, &source)
        });
        match initial {
            Ok(evaluation) => {
                if let Err(error) = self
                    .collect_worker_evaluation_with_parent_fetch_broker(
                        connection_id,
                        evaluation,
                        loader,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await
                {
                    self.workers.remove(&connection_id);
                    self.remove_worker_routes(connection_id);
                    return Err(error);
                }
            }
            Err(error) => {
                self.workers.remove(&connection_id);
                self.remove_worker_routes(connection_id);
                self.queue_error(connection_id, &resource.url, &error.to_string())?;
                return Ok(());
            }
        }
        let connect = self.evaluate_worker_with_loader(connection_id, loader, |worker| {
            worker.evaluate_shared_connect(connection_id, &transfer_port)
        });
        match connect {
            Ok(evaluation) => {
                if let Err(error) = self
                    .collect_worker_evaluation_with_parent_fetch_broker(
                        connection_id,
                        evaluation,
                        loader,
                        parent_fetch_broker,
                    )
                    .await
                {
                    self.workers.remove(&connection_id);
                    self.remove_worker_routes(connection_id);
                    return Err(error);
                }
            }
            Err(error) => {
                self.remove_transfer_routes(std::slice::from_ref(&transfer_port));
                self.workers.remove(&connection_id);
                self.remove_worker_routes(connection_id);
                self.queue_error(connection_id, &resource.url, &error.to_string())?;
            }
        }
        Ok(())
    }

    async fn post_message(
        &mut self,
        worker_id: u32,
        data: serde_json::Value,
        transfer_ports: Vec<NativeMessagePortTransfer>,
        object_urls: Vec<NativeObjectUrlTransfer>,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        if !self.workers.contains_key(&worker_id) {
            return Ok(());
        }
        self.register_page_transfers(worker_id, &transfer_ports)?;
        validate_native_object_url_transfers(&object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &data,
                "transfer_ports": &transfer_ports,
                "object_urls": &object_urls,
            }),
            "native Worker message",
        )?;
        let evaluation = self.evaluate_worker_with_loader(worker_id, loader, |worker| {
            worker.runtime.install_object_url_transfers(&object_urls)?;
            worker.evaluate_turn_with_event(
                worker_id,
                NativeWorkerDispatch::Message {
                    data: &data,
                    transfer_ports: &transfer_ports,
                    object_urls: &object_urls,
                },
            )
        });
        match evaluation {
            Ok(evaluation) => {
                self.collect_worker_evaluation_with_parent_fetch_broker(
                    worker_id,
                    evaluation,
                    loader,
                    parent_fetch_broker,
                )
                .await
            }
            Err(error) => {
                self.workers.remove(&worker_id);
                self.remove_worker_routes(worker_id);
                self.queue_error(worker_id, "", &error.to_string())
            }
        }
    }

    async fn collect_worker_evaluation(
        &mut self,
        worker_id: u32,
        evaluation: NativeScriptEvaluation,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.collect_worker_evaluation_with_parent_fetch_broker(worker_id, evaluation, loader, None)
            .await
    }

    async fn collect_worker_evaluation_with_parent_fetch_broker<'broker>(
        &mut self,
        worker_id: u32,
        evaluation: NativeScriptEvaluation,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<(), NativeEngineError> {
        let mut evaluations = VecDeque::from([(worker_id, evaluation)]);
        let mut evaluation_count = 0usize;
        while let Some((current_worker_id, evaluation)) = evaluations.pop_front() {
            evaluation_count = evaluation_count.saturating_add(1);
            if evaluation_count > MAX_NATIVE_WORKER_MESSAGES {
                return Err(NativeEngineError::limit(
                    "native Worker event-loop turns",
                    MAX_NATIVE_WORKER_MESSAGES,
                    evaluation_count,
                ));
            }
            let mut worker_script_error = evaluation.worker_script_error;
            if evaluation.top_level_await_pending
                && let Some(worker) = self.workers.get_mut(&current_worker_id)
            {
                worker.top_level_await_pending = true;
            }
            let mut closed = false;
            let message_port_commands = self
                .workers
                .get(&current_worker_id)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default();
            for command in message_port_commands {
                match command {
                    NativeScriptCommand::MessagePortPostMessage {
                        bridge_key,
                        data,
                        worker_id: Some(command_worker_id),
                        transfer_ports,
                        object_urls,
                    } if command_worker_id == current_worker_id => {
                        self.queue_page_message_port(
                            current_worker_id,
                            bridge_key,
                            data,
                            transfer_ports,
                            object_urls,
                        )?;
                    }
                    NativeScriptCommand::MessagePortClose {
                        bridge_key,
                        worker_id: Some(command_worker_id),
                    } if command_worker_id == current_worker_id => {
                        self.close_worker_message_port(current_worker_id, bridge_key)?;
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native Worker MessagePort command",
                            "worker emitted an invalid MessagePort host command",
                        ));
                    }
                }
            }
            for command in evaluation.commands {
                match command {
                    NativeScriptCommand::WorkerPostMessage {
                        worker_id: command_worker_id,
                        data,
                        transfer_ports,
                        object_urls,
                    } if command_worker_id == current_worker_id => {
                        self.register_worker_transfers(current_worker_id, &transfer_ports)?;
                        self.queue_message(NativeWorkerMessage {
                            worker_id: current_worker_id,
                            data,
                            error: None,
                            transfer_ports,
                            object_urls,
                        })?;
                    }
                    NativeScriptCommand::WorkerClose {
                        worker_id: command_worker_id,
                    } if command_worker_id == current_worker_id => closed = true,
                    NativeScriptCommand::WorkerScriptError {
                        worker_id: command_worker_id,
                        message,
                    } if command_worker_id == current_worker_id => {
                        if self
                            .workers
                            .get(&current_worker_id)
                            .is_none_or(|worker| worker.is_shared)
                        {
                            return Err(NativeEngineError::invalid(
                                "native Worker script error",
                                "callback error forwarding is only supported for dedicated Workers",
                            ));
                        }
                        self.queue_message(NativeWorkerMessage {
                            worker_id: current_worker_id,
                            data: serde_json::Value::Null,
                            error: Some(message),
                            transfer_ports: Vec::new(),
                            object_urls: Vec::new(),
                        })?;
                    }
                    NativeScriptCommand::MessagePortPostMessage {
                        bridge_key,
                        data,
                        worker_id: Some(command_worker_id),
                        transfer_ports,
                        object_urls,
                    } if command_worker_id == current_worker_id => {
                        self.queue_page_message_port(
                            current_worker_id,
                            bridge_key,
                            data,
                            transfer_ports,
                            object_urls,
                        )?;
                    }
                    NativeScriptCommand::Fetch {
                        worker_id: Some(command_worker_id),
                        ..
                    } if command_worker_id == current_worker_id => {
                        let resolved = self
                            .resolve_worker_fetch(
                                current_worker_id,
                                command,
                                loader,
                                parent_fetch_broker.as_deref_mut(),
                            )
                            .await?;
                        evaluations.push_back((current_worker_id, resolved));
                    }
                    NativeScriptCommand::FetchStreamRead {
                        stream_id,
                        worker_id: Some(command_worker_id),
                    } if command_worker_id == current_worker_id => {
                        self.process_worker_fetch_stream_command(
                            current_worker_id,
                            NativeScriptCommand::FetchStreamRead {
                                stream_id,
                                worker_id: Some(command_worker_id),
                            },
                        )?;
                    }
                    NativeScriptCommand::FetchStreamCancel {
                        stream_id,
                        worker_id: Some(command_worker_id),
                    } if command_worker_id == current_worker_id => {
                        self.process_worker_fetch_stream_command(
                            current_worker_id,
                            NativeScriptCommand::FetchStreamCancel {
                                stream_id,
                                worker_id: Some(command_worker_id),
                            },
                        )?;
                    }
                    command @ NativeScriptCommand::FetchUploadChunk {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | command @ NativeScriptCommand::FetchUploadEnd {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | command @ NativeScriptCommand::FetchUploadError {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | command @ NativeScriptCommand::FetchUploadCancel {
                        worker_id: Some(command_worker_id),
                        ..
                    } if command_worker_id == current_worker_id => {
                        self.process_worker_fetch_upload_command(current_worker_id, command)?;
                    }
                    command @ NativeScriptCommand::WebSocketOpen {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | command @ NativeScriptCommand::WebSocketSend {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | command @ NativeScriptCommand::WebSocketClose {
                        worker_id: Some(command_worker_id),
                        ..
                    } if command_worker_id == current_worker_id => {
                        self.queue_websocket_command(current_worker_id, command)?;
                    }
                    command @ NativeScriptCommand::EventSourceOpen {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | command @ NativeScriptCommand::EventSourceClose {
                        worker_id: Some(command_worker_id),
                        ..
                    } if command_worker_id == current_worker_id => {
                        self.queue_event_source_command(current_worker_id, command)?;
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native Worker command",
                            "worker emitted an invalid host command",
                        ));
                    }
                }
            }
            let top_level_await_result = self
                .workers
                .get(&current_worker_id)
                .filter(|worker| worker.top_level_await_pending)
                .map(|worker| worker.runtime.take_top_level_await_result());
            match top_level_await_result {
                Some(Ok(Some(_))) => {
                    if let Some(worker) = self.workers.get_mut(&current_worker_id) {
                        worker.top_level_await_pending = false;
                    }
                }
                Some(Ok(None)) | None => {}
                Some(Err(error)) => {
                    if let Some(worker) = self.workers.get_mut(&current_worker_id) {
                        worker.top_level_await_pending = false;
                    }
                    if worker_script_error.is_none() {
                        worker_script_error = Some(error.to_string());
                    }
                }
            }
            if let Some(error) = worker_script_error {
                self.queue_message(NativeWorkerMessage {
                    worker_id: current_worker_id,
                    data: serde_json::Value::Null,
                    error: Some(error),
                    transfer_ports: Vec::new(),
                    object_urls: Vec::new(),
                })?;
            }
            if closed {
                self.workers.remove(&current_worker_id);
                self.remove_worker_routes(current_worker_id);
            }
        }
        Ok(())
    }

    /// Route page-owned MessagePort commands into the worker endpoint named by
    /// the bridge key. Commands are applied after ordinary WorkerCreate and
    /// WorkerPostMessage commands from the same page turn, so a page can wire
    /// a channel and immediately send its first message deterministically.
    async fn load_dynamic_worker_module(
        &mut self,
        request: NativeDynamicWorkerModuleRequest<'_>,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<String, NativeEngineError> {
        let NativeDynamicWorkerModuleRequest {
            worker_id,
            request_id,
            worker_url,
            module_referrer,
            specifier,
            module_type,
            referrer_policy,
        } = request;
        let target = resolve_worker_module_specifier(module_referrer, specifier)?;
        let module_type = module_type.unwrap_or(NativeModuleResourceType::JavaScript);
        if module_type == NativeModuleResourceType::Unsupported {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "dynamic Worker module import attributes request an unsupported type"
                    .into(),
            });
        }
        let module_name = native_module_loader_name(&target, module_type);
        let existing_sources = {
            let worker = self.workers.get(&worker_id).ok_or_else(|| {
                NativeEngineError::invalid(
                    "native Worker dynamic import",
                    "worker no longer exists",
                )
            })?;
            if !worker
                .runtime
                .is_dynamic_import_referrer_allowed(module_referrer)?
            {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "dynamic module referrer is not an active Worker script".into(),
                });
            }
            let (sources, _) = worker.runtime.module_sources_snapshot()?;
            if sources.contains_key(&module_name) {
                return worker
                    .runtime
                    .register_dynamic_module_alias(request_id, &target);
            }
            sources
        };

        let worker_credentials_mode = self
            .workers
            .get(&worker_id)
            .filter(|worker| worker.is_shared && worker.is_module)
            .map(|worker| worker.credentials.clone());

        let referrer_policy =
            referrer_policy.unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin);
        let resource = load_worker_script_resource(
            loader,
            parent_fetch_broker.as_deref_mut(),
            NativeWorkerScriptResourceRequest {
                request_id,
                document_url: worker_url,
                href: &target,
                referrer_url: Some(module_referrer),
                max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                module_type: Some(module_type),
                credentials_mode: worker_credentials_mode.as_deref(),
                referrer_policy: Some(referrer_policy),
                shared_worker_module_entry: false,
                parent_loader_owns_network: self.parent_loader_owns_network,
            },
        )
        .await?
        .ok_or_else(|| NativeEngineError::Network {
            operation: "native Worker dynamic module import".into(),
            reason: "dynamic Worker module was blocked or unavailable".into(),
        })?;
        let graph = self
            .load_worker_module_graph(
                loader,
                NativeWorkerModuleGraphOptions {
                    owner_url: worker_url,
                    root_request_url: module_name.clone(),
                    credentials_mode: worker_credentials_mode.as_deref(),
                    request_id,
                    inherited_referrer_policy: Some(referrer_policy),
                    parent_loader_owns_network: self.parent_loader_owns_network,
                },
                resource,
                parent_fetch_broker,
            )
            .await?;
        let new_sources = graph
            .sources
            .into_iter()
            .filter(|(name, _)| !existing_sources.contains_key(name))
            .collect::<BTreeMap<_, _>>();
        let new_base_urls = graph
            .base_urls
            .into_iter()
            .filter(|(name, _)| new_sources.contains_key(name))
            .collect::<BTreeMap<_, _>>();
        let new_referrer_policies = graph
            .referrer_policies
            .into_iter()
            .filter(|(name, _)| new_sources.contains_key(name))
            .collect::<BTreeMap<_, _>>();
        let added_bytes = new_sources.values().map(String::len).sum::<usize>();
        let total_entries = existing_sources.len().saturating_add(new_sources.len());
        if total_entries > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "native Worker module graph entries",
                MAX_NATIVE_MODULE_IMPORTS,
                total_entries,
            ));
        }
        let total_bytes = existing_sources
            .values()
            .map(String::len)
            .sum::<usize>()
            .saturating_add(added_bytes);
        let byte_limit = MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS);
        if total_bytes > byte_limit {
            return Err(NativeEngineError::limit(
                "native Worker module graph bytes",
                byte_limit,
                total_bytes,
            ));
        }
        let worker = self.workers.get_mut(&worker_id).ok_or_else(|| {
            NativeEngineError::invalid("native Worker dynamic import", "worker no longer exists")
        })?;
        worker
            .runtime
            .extend_module_sources(new_sources.clone(), new_base_urls.clone())?;
        worker
            .runtime
            .extend_module_referrer_policies(new_referrer_policies.clone())?;
        worker.module_sources.extend(new_sources);
        worker.module_base_urls.extend(new_base_urls);
        worker
            .module_referrer_policies
            .extend(new_referrer_policies);
        worker
            .runtime
            .register_dynamic_module_alias(request_id, &target)
    }

    pub(super) fn queue_error(
        &mut self,
        worker_id: u32,
        worker_url: &str,
        error: &str,
    ) -> Result<(), NativeEngineError> {
        let mut message: String = error
            .chars()
            .take(MAX_NATIVE_SCRIPT_BYTES.min(4096))
            .collect();
        if message.is_empty() {
            message = "native Worker failed".into();
        }
        self.queue_message(NativeWorkerMessage {
            worker_id,
            data: serde_json::Value::Null,
            error: Some(if worker_url.is_empty() {
                message
            } else {
                format!("{message} ({worker_url})")
            }),
            transfer_ports: Vec::new(),
            object_urls: Vec::new(),
        })
    }

    pub(super) fn queue_message(
        &mut self,
        message: NativeWorkerMessage,
    ) -> Result<(), NativeEngineError> {
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_object_url_transfers(&message.object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "error": &message.error,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native Worker message",
        )?;
        if self.pending_messages.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Worker message queue",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_messages.len().saturating_add(1),
            ));
        }
        self.pending_messages.push_back(message);
        Ok(())
    }

    async fn load_worker_script_graph(
        &self,
        loader: &mut NativeResourceLoader,
        root: NativeScriptResource,
        options: NativeWorkerClassicGraphOptions,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeWorkerClassicScriptGraph, NativeEngineError> {
        let NativeWorkerClassicGraphOptions {
            dynamic_import_mode,
            worker_referrer_policy,
            request_id,
            parent_loader_owns_network,
        } = options;
        struct PendingWorkerScript {
            resource: NativeScriptResource,
            imports: Vec<String>,
            next_import: usize,
        }

        if root.body.is_empty() {
            return Err(NativeEngineError::invalid(
                "worker script source",
                "must not be empty",
            ));
        }
        let worker_url = root.url.clone();
        let root_imports = static_worker_import_specifiers(&root.body)?;
        let mut dynamic_import_referrers = BTreeSet::new();
        if matches!(
            dynamic_import_mode,
            NativeWorkerClassicDynamicImportMode::RewritePerScript
        ) {
            dynamic_import_referrers.insert(root.url.clone());
        }
        let mut stack = vec![PendingWorkerScript {
            resource: root,
            imports: root_imports,
            next_import: 0,
        }];
        let mut import_script_counts = BTreeMap::<String, usize>::new();
        let mut runtime_import_calls = 0usize;
        let mut import_edges = 0usize;
        let mut combined_source = String::new();
        while !stack.is_empty() {
            let next_import = stack.last_mut().and_then(|frame| {
                if frame.next_import >= frame.imports.len() {
                    return None;
                }
                let specifier = frame.imports[frame.next_import].clone();
                frame.next_import += 1;
                Some(specifier)
            });
            if let Some(specifier) = next_import {
                import_edges = import_edges.saturating_add(1);
                if import_edges > MAX_NATIVE_MODULE_IMPORTS {
                    return Err(NativeEngineError::limit(
                        "native worker importScripts graph",
                        MAX_NATIVE_MODULE_IMPORTS,
                        import_edges,
                    ));
                }
                let imported = load_worker_script_resource(
                    loader,
                    parent_fetch_broker.as_deref_mut(),
                    NativeWorkerScriptResourceRequest {
                        request_id,
                        document_url: &worker_url,
                        href: &specifier,
                        referrer_url: Some(&worker_url),
                        max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                        module_type: None,
                        credentials_mode: None,
                        referrer_policy: Some(worker_referrer_policy),
                        shared_worker_module_entry: false,
                        parent_loader_owns_network,
                    },
                )
                .await?
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "worker importScripts".into(),
                    reason: format!(
                        "worker importScripts dependency {specifier:?} was blocked or unavailable"
                    ),
                })?;
                if stack.iter().any(|entry| entry.resource.url == imported.url) {
                    return Err(NativeEngineError::Network {
                        operation: "worker importScripts".into(),
                        reason: "worker importScripts dependency graph contains a cycle".into(),
                    });
                }
                import_script_counts
                    .entry(specifier)
                    .and_modify(|count| *count = count.saturating_add(1))
                    .or_insert(1);
                let imports = static_worker_import_specifiers(&imported.body)?;
                stack.push(PendingWorkerScript {
                    resource: imported,
                    imports,
                    next_import: 0,
                });
                continue;
            }
            let frame = stack.pop().expect("worker script stack was non-empty");
            let script_source = match dynamic_import_mode {
                NativeWorkerClassicDynamicImportMode::RewritePerScript => {
                    let referrer = serde_json::to_string(&frame.resource.url).map_err(|_| {
                        NativeEngineError::Worker {
                            operation: "prepare Worker importScripts referrer".into(),
                            reason: "Worker importScripts referrer could not be encoded".into(),
                        }
                    })?;
                    let (source, rewritten_calls) =
                        rewrite_runtime_dynamic_module_imports_with_count(
                            &frame.resource.body,
                            &referrer,
                        )?;
                    runtime_import_calls = runtime_import_calls.saturating_add(rewritten_calls);
                    if runtime_import_calls > MAX_NATIVE_MODULE_IMPORTS {
                        return Err(NativeEngineError::limit(
                            "native worker runtime dynamic imports",
                            MAX_NATIVE_MODULE_IMPORTS,
                            runtime_import_calls,
                        ));
                    }
                    if rewritten_calls > 0 {
                        if !dynamic_import_referrers.contains(&frame.resource.url)
                            && dynamic_import_referrers.len() >= MAX_NATIVE_MODULE_IMPORTS
                        {
                            return Err(NativeEngineError::limit(
                                "dynamic module referrers",
                                MAX_NATIVE_MODULE_IMPORTS,
                                dynamic_import_referrers.len().saturating_add(1),
                            ));
                        }
                        dynamic_import_referrers.insert(frame.resource.url.clone());
                    }
                    source
                }
                NativeWorkerClassicDynamicImportMode::RejectPerScript => {
                    let (source, rewritten_calls) =
                        rewrite_dynamic_imports_as_rejected_with_count(&frame.resource.body)?;
                    runtime_import_calls = runtime_import_calls.saturating_add(rewritten_calls);
                    if runtime_import_calls > MAX_NATIVE_MODULE_IMPORTS {
                        return Err(NativeEngineError::limit(
                            "native service worker dynamic imports",
                            MAX_NATIVE_MODULE_IMPORTS,
                            runtime_import_calls,
                        ));
                    }
                    source
                }
            };
            let next_length = combined_source
                .len()
                .saturating_add(script_source.len())
                .saturating_add(2);
            if next_length > MAX_NATIVE_SCRIPT_BYTES {
                return Err(NativeEngineError::limit(
                    "combined worker importScripts source",
                    MAX_NATIVE_SCRIPT_BYTES,
                    next_length,
                ));
            }
            combined_source.push_str(&script_source);
            combined_source.push_str("\n;");
        }
        Ok(NativeWorkerClassicScriptGraph {
            source: combined_source,
            import_script_counts,
            dynamic_import_referrers,
        })
    }

    async fn load_worker_module_graph(
        &self,
        loader: &mut NativeResourceLoader,
        options: NativeWorkerModuleGraphOptions<'_>,
        root: NativeScriptResource,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeWorkerModuleGraph, NativeEngineError> {
        let NativeWorkerModuleGraphOptions {
            owner_url,
            root_request_url,
            credentials_mode,
            request_id,
            inherited_referrer_policy,
            parent_loader_owns_network,
        } = options;
        if root.body.is_empty() {
            return Err(NativeEngineError::invalid(
                "worker module source",
                "must not be empty",
            ));
        }
        let root_name = root_request_url;
        let root_base_url = root.url.clone();
        let root_referrer_policy = effective_worker_module_referrer_policy(
            inherited_referrer_policy,
            root.response_referrer_policy,
        );
        let mut sources = BTreeMap::from([(root_name.clone(), root.body.clone())]);
        let mut base_urls = BTreeMap::from([(root_name.clone(), root_base_url.clone())]);
        let mut referrer_policies = BTreeMap::new();
        if let Some(policy) = root_referrer_policy {
            referrer_policies.insert(root_name.clone(), policy);
        }
        let mut pending = VecDeque::from([(
            root_name.clone(),
            root_base_url.clone(),
            root.body,
            root_referrer_policy,
        )]);
        let mut total_bytes = sources.values().map(String::len).sum::<usize>();
        let mut import_edges = 0usize;
        while let Some((module_name, module_base_url, module_source, module_referrer_policy)) =
            pending.pop_front()
        {
            if module_name.starts_with(NATIVE_JSON_MODULE_NAME_PREFIX) {
                continue;
            }
            let requests = static_module_requests(&module_source)?;
            for request in requests {
                if request.module_type == NativeModuleResourceType::Unsupported {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason:
                            "Worker module import attributes request an unsupported module type"
                                .into(),
                    });
                }
                import_edges = import_edges.saturating_add(1);
                if import_edges > MAX_NATIVE_MODULE_IMPORTS {
                    return Err(NativeEngineError::limit(
                        "native Worker module imports",
                        MAX_NATIVE_MODULE_IMPORTS,
                        import_edges,
                    ));
                }
                let target = resolve_worker_module_specifier(&module_base_url, &request.specifier)?;
                let name = native_module_loader_name(&target, request.module_type);
                if sources.contains_key(&name) {
                    continue;
                }
                if sources.len() >= MAX_NATIVE_MODULE_IMPORTS {
                    return Err(NativeEngineError::limit(
                        "native Worker module graph entries",
                        MAX_NATIVE_MODULE_IMPORTS,
                        sources.len().saturating_add(1),
                    ));
                }
                let resource = load_worker_script_resource(
                    loader,
                    parent_fetch_broker.as_deref_mut(),
                    NativeWorkerScriptResourceRequest {
                        request_id,
                        document_url: owner_url,
                        href: &target,
                        referrer_url: Some(&module_base_url),
                        max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                        module_type: Some(request.module_type),
                        credentials_mode,
                        referrer_policy: module_referrer_policy,
                        shared_worker_module_entry: false,
                        parent_loader_owns_network,
                    },
                )
                .await?
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "native Worker module dependency".into(),
                    reason: format!(
                        "Worker module dependency {:?} was blocked or unavailable",
                        request.specifier
                    ),
                })?;
                if resource.body.is_empty() {
                    return Err(NativeEngineError::invalid(
                        "worker module source",
                        "must not be empty",
                    ));
                }
                total_bytes = total_bytes.saturating_add(resource.body.len());
                if total_bytes > MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS) {
                    return Err(NativeEngineError::limit(
                        "native Worker module graph bytes",
                        MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS),
                        total_bytes,
                    ));
                }
                let base_url = resource.url;
                let dependency_referrer_policy = effective_worker_module_referrer_policy(
                    module_referrer_policy,
                    resource.response_referrer_policy,
                );
                let source = resource.body;
                if sources.insert(name.clone(), source.clone()).is_none() {
                    base_urls.insert(name.clone(), base_url.clone());
                    if let Some(policy) = dependency_referrer_policy {
                        referrer_policies.insert(name.clone(), policy);
                    }
                    pending.push_back((name, base_url, source, dependency_referrer_policy));
                }
            }
        }
        Ok(NativeWorkerModuleGraph {
            root_name,
            root_base_url,
            sources,
            base_urls,
            referrer_policies,
        })
    }
}

pub(crate) async fn load_service_worker_entry(
    loader: &mut NativeResourceLoader,
    owner_url: &str,
    referrer_url: &str,
    script_url: &str,
    request_id: u32,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<Option<NativeScriptResource>, NativeEngineError> {
    if is_parent_owned_network_target(owner_url, script_url) {
        if let Some(parent_fetch_broker) = parent_fetch_broker {
            return parent_fetch_broker
                .load_worker_script(
                    request_id,
                    owner_url,
                    script_url,
                    None,
                    MAX_NATIVE_SCRIPT_BYTES,
                    None,
                    None,
                    referrer_policy,
                    false,
                    Some(referrer_url),
                )
                .await
                .map(|(resource, _)| resource);
        }
        return Err(missing_parent_network_authority(
            "Service Worker script load",
        ));
    }
    loader
        .load_worker_async_with_referrer_policy(
            referrer_url,
            script_url,
            MAX_NATIVE_SCRIPT_BYTES,
            referrer_policy,
        )
        .await
}

pub(crate) async fn load_service_worker_source(
    loader: &mut NativeResourceLoader,
    owner_url: &str,
    root_request_url: String,
    resource: NativeScriptResource,
    is_module: bool,
    request_id: u32,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<
    (
        String,
        BTreeMap<String, usize>,
        Option<NativeWorkerModuleGraph>,
        NativeFetchReferrerPolicy,
    ),
    NativeEngineError,
> {
    let registry = NativeWorkerRegistry::new();
    let worker_referrer_policy = effective_worker_global_referrer_policy(
        &resource.url,
        None,
        resource.response_referrer_policy,
    );
    if is_module {
        let module_graph = registry
            .load_worker_module_graph(
                loader,
                NativeWorkerModuleGraphOptions {
                    owner_url,
                    root_request_url,
                    credentials_mode: None,
                    request_id,
                    inherited_referrer_policy: Some(worker_referrer_policy),
                    parent_loader_owns_network: registry.parent_loader_owns_network,
                },
                resource.clone(),
                parent_fetch_broker.as_deref_mut(),
            )
            .await?;
        Ok((
            resource.body,
            BTreeMap::new(),
            Some(module_graph),
            worker_referrer_policy,
        ))
    } else {
        let graph = registry
            .load_worker_script_graph(
                loader,
                resource,
                NativeWorkerClassicGraphOptions {
                    dynamic_import_mode: NativeWorkerClassicDynamicImportMode::RejectPerScript,
                    worker_referrer_policy,
                    request_id,
                    parent_loader_owns_network: registry.parent_loader_owns_network,
                },
                parent_fetch_broker,
            )
            .await?;
        Ok((
            graph.source,
            graph.import_script_counts,
            None,
            worker_referrer_policy,
        ))
    }
}
