use super::*;

mod evaluation;

pub(crate) struct NativePageEventContext<'a> {
    pub(crate) document: &'a NativeDocument,
    pub(crate) document_url: &'a str,
    pub(crate) origin: &'a NativeOrigin,
    pub(crate) viewport: Viewport,
    pub(crate) csp_violations: &'a [NativeCspViolation],
}

struct NativePageEvaluationContext<'a> {
    document: &'a NativeDocument,
    document_url: &'a str,
    origin: &'a NativeOrigin,
    viewport: Viewport,
}

pub(crate) struct NativeWorkerModuleEvaluation<'a> {
    pub(crate) worker_id: u32,
    pub(crate) worker_url: &'a str,
    pub(crate) module_name: &'a str,
    pub(crate) module_base_url: &'a str,
    pub(crate) source: &'a str,
    pub(crate) module_sources: &'a BTreeMap<String, String>,
    pub(crate) module_base_urls: &'a BTreeMap<String, String>,
    pub(crate) module_referrer_policies: &'a BTreeMap<String, NativeFetchReferrerPolicy>,
    pub(crate) referrer_policy: Option<NativeFetchReferrerPolicy>,
    pub(crate) capture_startup_runtime_errors: bool,
}

pub(crate) struct NativeServiceWorkerMessageEvent<'a> {
    pub(crate) worker_id: u32,
    pub(crate) worker_url: &'a str,
    pub(crate) data: &'a serde_json::Value,
    pub(crate) transfer_ports: &'a [NativeMessagePortTransfer],
    pub(crate) source: &'a serde_json::Value,
    pub(crate) source_origin: &'a str,
    pub(crate) is_module: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct NativeWorkerExecutionMode {
    pub(crate) service_worker: bool,
    pub(crate) await_promise: bool,
}

pub(crate) struct NativeWorkerSourceIdentity<'a> {
    pub(crate) worker_id: u32,
    pub(crate) worker_url: &'a str,
    pub(crate) module_name: Option<&'a str>,
}

impl NativeJavaScriptRuntime {
    pub(crate) fn new_with_context_id(
        context_id: impl Into<String>,
    ) -> Result<Self, NativeEngineError> {
        Self::new_with_context_metadata(context_id, "", None, "", "")
    }

    pub(crate) fn new_with_context_metadata(
        context_id: impl Into<String>,
        window_name: impl Into<String>,
        opener_context_id: Option<&str>,
        opener_window_name: impl Into<String>,
        opener_url: impl Into<String>,
    ) -> Result<Self, NativeEngineError> {
        let context_id = context_id.into();
        let window_name = window_name.into();
        let opener_window_name = opener_window_name.into();
        let opener_url = opener_url.into();
        validate_context_id(&context_id)?;
        validate_window_name(&window_name)?;
        validate_window_name(&opener_window_name)?;
        if !opener_url.is_empty() {
            validate_url_text("opener URL", &opener_url)?;
        }
        if let Some(opener_context_id) = opener_context_id {
            validate_context_id(opener_context_id)?;
        }
        let runtime = Runtime::new().map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript runtime".into(),
            reason: "native JavaScript runtime could not be created".into(),
        })?;
        let module_sources = Arc::new(Mutex::new(BTreeMap::new()));
        let module_base_urls = Arc::new(Mutex::new(BTreeMap::new()));
        let module_referrer_policies = Arc::new(Mutex::new(BTreeMap::new()));
        let module_import_map = Arc::new(Mutex::new(NativeModuleImportMap::default()));
        let dynamic_module_aliases = Arc::new(Mutex::new(BTreeMap::new()));
        let dynamic_import_mode = Arc::new(Mutex::new(NativeModuleDynamicImportMode::Preserve));
        runtime.set_loader(
            NativeModuleResolver {
                import_map: Arc::clone(&module_import_map),
                module_base_urls: Arc::clone(&module_base_urls),
                dynamic_module_aliases: Arc::clone(&dynamic_module_aliases),
            },
            NativeModuleLoader {
                sources: Arc::clone(&module_sources),
                module_base_urls: Arc::clone(&module_base_urls),
                module_referrer_policies: Arc::clone(&module_referrer_policies),
                dynamic_import_mode: Arc::clone(&dynamic_import_mode),
            },
        );
        runtime.set_memory_limit(NATIVE_SCRIPT_MEMORY_BYTES);
        runtime.set_max_stack_size(NATIVE_SCRIPT_STACK_BYTES);
        let deadline = Arc::new(Mutex::new(None));
        let interrupt_deadline = Arc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt_deadline
                .lock()
                .ok()
                .and_then(|deadline| *deadline)
                .is_some_and(|deadline| Instant::now() >= deadline)
        })));
        let unhandled_promise_rejections =
            Arc::new(Mutex::new(NativeUnhandledPromiseRejections::default()));
        let rejection_queue = Arc::clone(&unhandled_promise_rejections);
        runtime.set_host_promise_rejection_tracker(Some(Box::new(
            move |ctx, promise, reason, is_handled| {
                let key = format!("{promise:?}");
                let fallback = format!("{reason:?}");
                let reason = Coerced::<std::string::String>::from_js(&ctx, reason)
                    .map(|value| value.0)
                    .unwrap_or(fallback);
                let reason = bounded_unhandled_promise_rejection_reason(reason);
                if let Ok(mut queue) = rejection_queue.lock() {
                    if is_handled {
                        if queue.pending.remove(&key).is_none()
                            && let Some((_, reason)) = queue.reported.remove(&key)
                            && queue.handled.len() < MAX_NATIVE_UNHANDLED_REJECTIONS
                        {
                            let order = queue.next_order;
                            queue.next_order = queue.next_order.saturating_add(1);
                            queue.handled.insert(key, (order, reason));
                        }
                    } else if queue.pending.len() < MAX_NATIVE_UNHANDLED_REJECTIONS
                        || queue.pending.contains_key(&key)
                    {
                        let order = queue
                            .pending
                            .get(&key)
                            .map(|(order, _)| *order)
                            .unwrap_or_else(|| {
                                let order = queue.next_order;
                                queue.next_order = queue.next_order.saturating_add(1);
                                order
                            });
                        queue.pending.insert(key, (order, reason));
                    }
                }
            },
        )));
        let context = Context::full(&runtime).map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript context".into(),
            reason: "native JavaScript context could not be created".into(),
        })?;
        let dialog_handler = Arc::new(Mutex::new(None::<NativeDialogHandler>));
        let dialog_url = Arc::new(Mutex::new(String::new()));
        context
            .with(|ctx| {
                install_native_html_fragment_parser(ctx.clone())?;
                install_native_html_document_parser(ctx.clone())?;
                install_native_html_document_decoder(ctx.clone())?;
                ctx.eval::<(), _>(
                    r#"Object.defineProperty(globalThis, "__glassParseJSONModule", {
                      value: JSON.parse,
                      writable: false,
                      configurable: false,
                      enumerable: false,
                    });"#,
                )?;
                Ok::<(), Error>(())
            })
            .map_err(|_| NativeEngineError::Worker {
                operation: "initialize native JavaScript host functions".into(),
                reason: "native JavaScript host functions could not be installed".into(),
            })?;
        Ok(Self {
            runtime,
            context,
            deadline,
            environment: Arc::new(Mutex::new(NativeEnvironmentOverrides::default())),
            module_sources,
            module_base_urls,
            module_referrer_policies,
            worker_global_referrer_policy: None,
            module_import_map,
            dynamic_module_aliases,
            dynamic_import_mode,
            dynamic_import_referrers: Arc::new(Mutex::new(BTreeSet::new())),
            sync_xhr_loader: Arc::new(Mutex::new(None)),
            sync_xhr_loader_used: Arc::new(AtomicBool::new(false)),
            timer_pump_enabled: Arc::new(Mutex::new(true)),
            storage: Arc::new(Mutex::new(NativeWebStorageState::default())),
            indexed_db: Arc::new(Mutex::new(NativeIndexedDbOrigin::default())),
            storage_changes: Arc::new(Mutex::new(Vec::new())),
            pending_storage_events: Arc::new(Mutex::new(Vec::new())),
            cookie: Arc::new(Mutex::new(String::new())),
            cookie_updates: Arc::new(Mutex::new(Vec::new())),
            parent_brokered_cookie_updates: Arc::new(Mutex::new(VecDeque::new())),
            unhandled_promise_rejections,
            dialog_events: Arc::new(Mutex::new(Vec::new())),
            dialog_handler,
            dialog_url,
            popup_events: Arc::new(Mutex::new(Vec::new())),
            post_message_events: Arc::new(Mutex::new(Vec::new())),
            window_close_events: Arc::new(Mutex::new(Vec::new())),
            window_navigation_events: Arc::new(Mutex::new(Vec::new())),
            frame_script_events: Arc::new(Mutex::new(Vec::new())),
            worker_commands: Arc::new(Mutex::new(Vec::new())),
            message_port_commands: Arc::new(Mutex::new(Vec::new())),
            service_worker_commands: Arc::new(Mutex::new(Vec::new())),
            document_creation_url_state: Arc::new(Mutex::new(None)),
            service_worker_registrations: Arc::new(Mutex::new(Vec::new())),
            service_worker_clients: Arc::new(Mutex::new(Vec::new())),
            inline_script_policy: Arc::new(Mutex::new(NativeInlineScriptPolicy::default())),
            processed_inline_csp_meta_nodes: Arc::new(Mutex::new(BTreeSet::new())),
            pending_window_proxy_updates: Arc::new(Mutex::new(Vec::new())),
            frame_script_bindings: Arc::new(Mutex::new(Vec::new())),
            frame_script_context: Arc::new(Mutex::new(None)),
            frame_id: Arc::new(Mutex::new(context_id.clone())),
            scroll_offset: Arc::new(Mutex::new(NativePoint { x: 0, y: 0 })),
            nested_scroll_offsets: Arc::new(Mutex::new(BTreeMap::new())),
            history_state: Arc::new(Mutex::new(serde_json::Value::Null)),
            history_length: Arc::new(Mutex::new(1)),
            window_name: Arc::new(Mutex::new(window_name)),
            opener_context_id: opener_context_id.map(str::to_owned),
            opener_window_name,
            opener_url,
            storage_context_id: context_id,
            ready_state: "complete".into(),
            clock_origin: Instant::now(),
        })
    }

    pub(crate) fn set_worker_global_referrer_policy(&mut self, policy: NativeFetchReferrerPolicy) {
        self.worker_global_referrer_policy = Some(policy);
    }

    fn document_creation_url_for(
        &self,
        document: &NativeDocument,
        document_url: &str,
    ) -> Result<String, NativeEngineError> {
        let generation = document.generation();
        let mut state =
            self.document_creation_url_state
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "read Document creation URL".into(),
                    reason: "Document creation URL state is unavailable".into(),
                })?;
        if let Some((current_generation, creation_url)) = state.as_ref()
            && *current_generation == generation
        {
            return Ok(creation_url.clone());
        }
        validate_url_text("native Document creation URL", document_url)?;
        let creation_url = document_url.to_owned();
        *state = Some((generation, creation_url.clone()));
        Ok(creation_url)
    }

    pub(crate) fn new_with_context_metadata_and_dialog_handler(
        context_id: impl Into<String>,
        window_name: impl Into<String>,
        opener_context_id: Option<&str>,
        opener_window_name: impl Into<String>,
        opener_url: impl Into<String>,
        dialog_handler: NativeDialogHandler,
    ) -> Result<Self, NativeEngineError> {
        let runtime = Self::new_with_context_metadata(
            context_id,
            window_name,
            opener_context_id,
            opener_window_name,
            opener_url,
        )?;
        runtime.set_dialog_handler(dialog_handler)?;
        Ok(runtime)
    }

    pub(crate) fn set_dialog_handler(
        &self,
        handler: NativeDialogHandler,
    ) -> Result<(), NativeEngineError> {
        *self
            .dialog_handler
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "install native dialog handler".into(),
                reason: "native dialog handler state is unavailable".into(),
            })? = Some(handler);
        Ok(())
    }

    fn set_dialog_url(&self, url: &str) -> Result<(), NativeEngineError> {
        validate_url_text("native dialog URL", url)?;
        *self
            .dialog_url
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "set native dialog URL".into(),
                reason: "native dialog URL state is unavailable".into(),
            })? = url.to_owned();
        Ok(())
    }

    pub(crate) fn set_environment(&self, environment: NativeEnvironmentOverrides) {
        if let Ok(mut current) = self.environment.lock() {
            *current = environment;
        }
    }

    /// Give standalone synchronous XHR a bounded loader snapshot. A content
    /// worker installs a parent broker instead, so its XHR never uses this
    /// child-local copy.
    pub(crate) fn set_sync_xhr_loader(&self, loader: &NativeResourceLoader) {
        self.sync_xhr_loader_used.store(false, Ordering::Release);
        if let Ok(mut current) = self.sync_xhr_loader.lock() {
            *current = Some(loader.clone());
        }
    }

    pub(crate) fn take_sync_xhr_loader(&self) -> Option<NativeResourceLoader> {
        let used = self.sync_xhr_loader_used.swap(false, Ordering::AcqRel);
        let loader = self
            .sync_xhr_loader
            .lock()
            .ok()
            .and_then(|mut current| current.take());
        used.then_some(loader).flatten()
    }

    /// Snapshot the live Blob URLs referenced by a structured message. The
    /// message itself contains only strings; finite resource copies are kept
    /// in a separate envelope so a destination realm can install them before
    /// its normal structured-clone decoder runs.
    fn object_url_transfers_in_context(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        data: &serde_json::Value,
    ) -> Result<Vec<NativeObjectUrlTransfer>, NativeEngineError> {
        const MAX_OBJECT_URL_SCAN_DEPTH: usize = 128;
        let mut values = vec![(data, 0usize)];
        let mut seen = BTreeSet::new();
        let mut transfers = Vec::new();
        while let Some((value, depth)) = values.pop() {
            if depth > MAX_OBJECT_URL_SCAN_DEPTH {
                return Err(NativeEngineError::limit(
                    "native object URL message depth",
                    MAX_OBJECT_URL_SCAN_DEPTH,
                    depth,
                ));
            }
            match value {
                serde_json::Value::String(href)
                    if href
                        .get(..5)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
                        && href.len() <= MAX_NATIVE_SCRIPT_BYTES
                        && Url::parse(href).is_ok()
                        && seen.insert(href.clone()) =>
                {
                    if let Some(resource) = self.object_url_resource_in_context(ctx, href)? {
                        transfers.push(NativeObjectUrlTransfer {
                            href: href.clone(),
                            resource,
                        });
                        if transfers.len() > MAX_NATIVE_OBJECT_URL_TRANSFERS {
                            return Err(NativeEngineError::limit(
                                "native object URL transfers",
                                MAX_NATIVE_OBJECT_URL_TRANSFERS,
                                transfers.len(),
                            ));
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for item in items {
                        values.push((item, depth.saturating_add(1)));
                    }
                }
                serde_json::Value::Object(object) => {
                    for item in object.values() {
                        values.push((item, depth.saturating_add(1)));
                    }
                }
                _ => {}
            }
        }
        validate_native_object_url_transfers(&transfers)?;
        Ok(transfers)
    }

    fn attach_object_url_transfers(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        mut command: NativeScriptCommand,
    ) -> Result<NativeScriptCommand, NativeEngineError> {
        match &mut command {
            NativeScriptCommand::WorkerPostMessage {
                data, object_urls, ..
            }
            | NativeScriptCommand::ServiceWorkerClientPostMessage {
                data, object_urls, ..
            } => {
                *object_urls = self.object_url_transfers_in_context(ctx, data)?;
            }
            _ => {}
        }
        Ok(command)
    }

    /// Install bounded Blob URL snapshots in this realm before a message is
    /// decoded. Installation replaces only the destination's entry for the
    /// transferred URL; source-realm revocation remains independent.
    pub(crate) fn install_object_url_transfers(
        &self,
        transfers: &[NativeObjectUrlTransfer],
    ) -> Result<(), NativeEngineError> {
        validate_native_object_url_transfers(transfers)?;
        if transfers.is_empty() {
            return Ok(());
        }
        let payload = serde_json::to_value(transfers).map_err(|_| NativeEngineError::Worker {
            operation: "serialize native object URL transfers".into(),
            reason: "native object URL transfers could not be serialized".into(),
        })?;
        self.context.with(|ctx| {
            let payload = native_message_payload(&ctx, &payload, "native object URL transfers")?;
            let installer: Function = ctx
                .globals()
                .get("__glassInstallObjectUrlTransfers")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "install native object URL transfers".into(),
                    reason: format!(
                        "native object URL transfer installer was unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            installer
                .call::<_, Value>((payload,))
                .map_err(|error| NativeEngineError::Worker {
                    operation: "install native object URL transfers".into(),
                    reason: format!(
                        "native object URL transfers could not be installed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            Ok(())
        })
    }

    /// Snapshot a live page Blob URL before a full navigation replaces the
    /// realm. The JavaScript registry remains the lifetime authority; Rust
    /// only receives a bounded byte copy for the next document owner.
    pub(crate) fn object_url_resource(
        &self,
        href: &str,
    ) -> Result<Option<NativeObjectUrlResource>, NativeEngineError> {
        validate_url_text("native object URL", href)?;
        self.context
            .with(|ctx| self.object_url_resource_in_context(&ctx, href))
    }

    fn object_url_resource_in_context(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        href: &str,
    ) -> Result<Option<NativeObjectUrlResource>, NativeEngineError> {
        validate_url_text("native object URL", href)?;
        let getter: Function =
            ctx.globals()
                .get("__glassGetObjectUrlResource")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "read native object URL".into(),
                    reason: format!(
                        "native object URL registry was unavailable: {}",
                        CaughtError::from_error(ctx, error)
                    ),
                })?;
        let value = getter
            .call::<_, Value>((href.to_owned(),))
            .map_err(|error| NativeEngineError::Worker {
                operation: "read native object URL".into(),
                reason: format!(
                    "native object URL registry read failed: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
        let json = ctx
            .json_stringify(value)
            .map_err(|error| NativeEngineError::Worker {
                operation: "serialize native object URL".into(),
                reason: format!(
                    "native object URL could not be serialized: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
        let Some(json) = json else {
            return Ok(None);
        };
        let json = json.to_string().map_err(|_| NativeEngineError::Worker {
            operation: "serialize native object URL".into(),
            reason: "native object URL could not be converted to UTF-8".into(),
        })?;
        if json == "null" {
            return Ok(None);
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct NativeObjectUrlPayload {
            #[serde(default)]
            content_type: Option<String>,
            body_base64: String,
        }
        let payload = serde_json::from_str::<NativeObjectUrlPayload>(&json).map_err(|_| {
            NativeEngineError::Worker {
                operation: "decode native object URL".into(),
                reason: "native object URL registry returned malformed data".into(),
            }
        })?;
        if payload.body_base64.len() > MAX_NATIVE_SCRIPT_BYTES.saturating_mul(2) {
            return Err(NativeEngineError::limit(
                "native object URL payload",
                MAX_NATIVE_SCRIPT_BYTES,
                payload.body_base64.len(),
            ));
        }
        let body = base64::engine::general_purpose::STANDARD
            .decode(payload.body_base64)
            .map_err(|_| NativeEngineError::Worker {
                operation: "decode native object URL".into(),
                reason: "native object URL body was not valid base64".into(),
            })?;
        if body.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "native object URL payload",
                MAX_NATIVE_SCRIPT_BYTES,
                body.len(),
            ));
        }
        if payload
            .content_type
            .as_ref()
            .is_some_and(|content_type| content_type.len() > MAX_NATIVE_SCRIPT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "native object URL content type",
                MAX_NATIVE_SCRIPT_BYTES,
                payload.content_type.as_ref().map_or(0, String::len),
            ));
        }
        Ok(Some(NativeObjectUrlResource {
            content_type: payload.content_type,
            body,
        }))
    }

    pub(crate) fn set_inline_script_policy(&self, policy: NativeInlineScriptPolicy) {
        if let Ok(mut current) = self.inline_script_policy.lock() {
            *current = policy;
        }
    }

    pub(crate) fn mark_processed_inline_csp_meta_nodes(
        &self,
        node_indexes: impl IntoIterator<Item = u32>,
    ) {
        if let Ok(mut current) = self.processed_inline_csp_meta_nodes.lock() {
            current.extend(node_indexes);
        }
    }

    fn environment(&self) -> NativeEnvironmentOverrides {
        self.environment
            .lock()
            .map(|environment| environment.clone())
            .unwrap_or_default()
    }

    pub(crate) fn window_name(&self) -> String {
        self.window_name
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    pub(crate) fn opener_context_id(&self) -> Option<&str> {
        self.opener_context_id.as_deref()
    }

    pub(crate) fn opener_window_name(&self) -> &str {
        &self.opener_window_name
    }

    pub(crate) fn set_storage_state(&self, state: NativeWebStorageState) {
        if let Ok(mut current) = self.storage.lock() {
            *current = state;
        }
    }

    pub(crate) fn replace_storage_state(&self, state: NativeWebStorageState) {
        self.set_storage_state(state);
        if let Ok(mut pending) = self.pending_storage_events.lock() {
            pending.clear();
        }
    }

    pub(crate) fn storage_state(&self) -> NativeWebStorageState {
        self.storage
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_indexed_db_state(&self, state: NativeIndexedDbOrigin) {
        if state.validate().is_err() {
            return;
        }
        if let Ok(mut current) = self.indexed_db.lock() {
            *current = state;
        }
    }

    pub(crate) fn set_timer_pump_enabled(&self, enabled: bool) {
        if let Ok(mut current) = self.timer_pump_enabled.lock() {
            *current = enabled;
        }
    }

    pub(crate) fn suspend_timer_pump(&self) -> NativeTimerPumpGuard<'_> {
        let previous = self.timer_pump_enabled();
        self.set_timer_pump_enabled(false);
        NativeTimerPumpGuard {
            runtime: self,
            previous,
        }
    }

    fn timer_pump_enabled(&self) -> bool {
        self.timer_pump_enabled
            .lock()
            .map(|enabled| *enabled)
            .unwrap_or(true)
    }

    pub(crate) fn indexed_db_state(&self) -> NativeIndexedDbOrigin {
        self.indexed_db
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    fn take_unhandled_promise_rejections(&self) -> Vec<String> {
        let Ok(mut queue) = self.unhandled_promise_rejections.lock() else {
            return Vec::new();
        };
        let mut values = std::mem::take(&mut queue.pending)
            .into_iter()
            .collect::<Vec<_>>();
        values.sort_unstable_by_key(|(_, (order, _))| *order);
        let mut reasons = Vec::with_capacity(values.len());
        for (key, (order, reason)) in values {
            if queue.reported.len() >= MAX_NATIVE_UNHANDLED_REJECTIONS
                && let Some(key) = queue
                    .reported
                    .iter()
                    .min_by_key(|(_, (reported_order, _))| *reported_order)
                    .map(|(key, _)| key.clone())
            {
                queue.reported.remove(&key);
            }
            queue.reported.insert(key, (order, reason.clone()));
            reasons.push(reason);
        }
        reasons
    }

    fn take_handled_promise_rejections(&self) -> Vec<String> {
        let Ok(mut queue) = self.unhandled_promise_rejections.lock() else {
            return Vec::new();
        };
        let mut values = std::mem::take(&mut queue.handled)
            .into_values()
            .collect::<Vec<_>>();
        values.sort_unstable_by_key(|(order, _)| *order);
        values.into_iter().map(|(_, reason)| reason).collect()
    }

    pub(crate) fn take_storage_changes(&self) -> Vec<NativeStorageEvent> {
        self.storage_changes
            .lock()
            .map(|mut changes| std::mem::take(&mut *changes))
            .unwrap_or_default()
    }

    pub(crate) fn set_storage_events(
        &self,
        events: Vec<NativeStorageEvent>,
    ) -> Result<(), NativeEngineError> {
        if events.is_empty() {
            return Ok(());
        }
        let mut pending =
            self.pending_storage_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue native storage events".into(),
                    reason: "native storage event queue is unavailable".into(),
                })?;
        let next_len = pending.len().saturating_add(events.len());
        if next_len > MAX_NATIVE_STORAGE_EVENTS {
            return Err(NativeEngineError::limit(
                "native storage events",
                MAX_NATIVE_STORAGE_EVENTS,
                next_len,
            ));
        }
        pending.extend(events);
        Ok(())
    }

    fn take_storage_events(&self) -> Vec<NativeStorageEvent> {
        self.pending_storage_events
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }

    pub(crate) fn set_cookie_state(&self, value: impl Into<String>) {
        let value = value.into();
        if value.len() > crate::limits::MAX_TEXT_BYTES {
            return;
        }
        if let Ok(mut current) = self.cookie.lock() {
            *current = value;
        }
    }

    fn cookie_state(&self) -> String {
        self.cookie
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    pub(crate) fn take_cookie_updates(&self) -> Vec<String> {
        let updates = self
            .cookie_updates
            .lock()
            .map(|mut updates| std::mem::take(&mut *updates))
            .unwrap_or_default();
        let Ok(mut brokered) = self.parent_brokered_cookie_updates.lock() else {
            return updates;
        };
        let mut unbrokered = Vec::with_capacity(updates.len());
        for update in updates {
            if brokered.front().is_some_and(|value| value == &update) {
                brokered.pop_front();
            } else {
                unbrokered.push(update);
            }
        }
        unbrokered
    }

    pub(crate) fn take_dialog_events(&self) -> Vec<NativeDialog> {
        self.dialog_events
            .lock()
            .map(|mut dialogs| std::mem::take(&mut *dialogs))
            .unwrap_or_default()
    }

    pub(crate) fn take_popup_events(&self) -> Vec<NativePopupRequest> {
        self.popup_events
            .lock()
            .map(|mut popups| std::mem::take(&mut *popups))
            .unwrap_or_default()
    }

    pub(crate) fn take_post_message_events(&self) -> Vec<NativePostMessageRequest> {
        self.post_message_events
            .lock()
            .map(|mut messages| std::mem::take(&mut *messages))
            .unwrap_or_default()
    }

    pub(crate) fn take_window_close_events(&self) -> Vec<NativeWindowCloseRequest> {
        self.window_close_events
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default()
    }

    pub(crate) fn take_window_navigation_events(&self) -> Vec<NativeWindowNavigationRequest> {
        self.window_navigation_events
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default()
    }

    pub(crate) fn take_frame_script_events(&self) -> Vec<NativeFrameScriptRequest> {
        self.frame_script_events
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default()
    }

    pub(crate) fn take_worker_commands(&self) -> Vec<NativeScriptCommand> {
        self.worker_commands
            .lock()
            .map(|mut commands| std::mem::take(&mut *commands))
            .unwrap_or_default()
    }

    pub(crate) fn take_message_port_commands(&self) -> Vec<NativeScriptCommand> {
        self.message_port_commands
            .lock()
            .map(|mut commands| std::mem::take(&mut *commands))
            .unwrap_or_default()
    }

    pub(crate) fn take_service_worker_commands(&self) -> Vec<NativeScriptCommand> {
        self.service_worker_commands
            .lock()
            .map(|mut commands| std::mem::take(&mut *commands))
            .unwrap_or_default()
    }

    pub(crate) fn set_service_worker_registrations(
        &self,
        registrations: Vec<NativeServiceWorkerRegistrationState>,
    ) {
        if let Ok(mut current) = self.service_worker_registrations.lock() {
            *current = registrations;
        }
    }

    fn service_worker_registrations(&self) -> Vec<NativeServiceWorkerRegistrationState> {
        self.service_worker_registrations
            .lock()
            .map(|registrations| registrations.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_service_worker_clients(&self, clients: Vec<serde_json::Value>) {
        if let Ok(mut current) = self.service_worker_clients.lock() {
            *current = clients;
        }
    }

    fn service_worker_clients(&self) -> Vec<serde_json::Value> {
        self.service_worker_clients
            .lock()
            .map(|clients| clients.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_frame_script_bindings(&self, bindings: Vec<NativeFrameScriptBinding>) {
        if let Ok(mut current) = self.frame_script_bindings.lock() {
            *current = bindings;
        }
    }

    pub(crate) fn set_frame_script_context(&self, context: Option<NativeFrameScriptContext>) {
        if let Ok(mut current) = self.frame_script_context.lock() {
            *current = context;
        }
    }

    pub(crate) fn set_frame_id(&self, frame_id: String) {
        if validate_context_id(&frame_id).is_err() {
            return;
        }
        if let Ok(mut current) = self.frame_id.lock() {
            *current = frame_id;
        }
    }

    pub(crate) fn set_scroll_offset(&self, scroll_offset: NativePoint) {
        if let Ok(mut current) = self.scroll_offset.lock() {
            *current = scroll_offset;
        }
    }

    fn scroll_offset(&self) -> NativePoint {
        self.scroll_offset
            .lock()
            .map(|offset| *offset)
            .unwrap_or(NativePoint { x: 0, y: 0 })
    }

    pub(crate) fn set_nested_scroll_offsets(&self, offsets: BTreeMap<u32, NativePoint>) {
        if let Ok(mut current) = self.nested_scroll_offsets.lock() {
            *current = offsets;
        }
    }

    fn nested_scroll_offsets(&self) -> BTreeMap<u32, NativePoint> {
        self.nested_scroll_offsets
            .lock()
            .map(|offsets| offsets.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_history_state(&self, state: serde_json::Value) {
        if let Ok(mut current) = self.history_state.lock() {
            *current = state;
        }
    }

    pub(crate) fn set_history_length(&self, length: usize) {
        if let Ok(mut current) = self.history_length.lock() {
            *current = length.max(1);
        }
    }

    fn history_state(&self) -> serde_json::Value {
        self.history_state
            .lock()
            .map(|state| state.clone())
            .unwrap_or(serde_json::Value::Null)
    }

    pub(crate) fn history_length(&self) -> usize {
        self.history_length
            .lock()
            .map(|length| (*length).max(1))
            .unwrap_or(1)
    }

    fn frame_id(&self) -> String {
        self.frame_id
            .lock()
            .map(|frame_id| frame_id.clone())
            .unwrap_or_else(|_| self.storage_context_id.clone())
    }

    fn frame_script_context(&self) -> Option<NativeFrameScriptContext> {
        self.frame_script_context
            .lock()
            .ok()
            .and_then(|context| context.clone())
    }

    fn frame_script_bindings(&self) -> Vec<NativeFrameScriptBinding> {
        self.frame_script_bindings
            .lock()
            .map(|bindings| bindings.clone())
            .unwrap_or_default()
    }

    pub(crate) fn sync_window_proxies(
        &self,
        updates: &[NativeWindowProxyUpdate],
    ) -> Result<(), NativeEngineError> {
        if updates.is_empty() {
            return Ok(());
        }
        if updates.len() > crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native WindowProxy updates",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                updates.len(),
            ));
        }
        for update in updates {
            if update.cache_key.len() > crate::limits::MAX_JSON_BYTES
                || update.href.len() > crate::limits::MAX_TEXT_BYTES
                || update.name.len() > MAX_NATIVE_WINDOW_NAME_BYTES
            {
                return Err(NativeEngineError::limit(
                    "native WindowProxy update",
                    crate::limits::MAX_JSON_BYTES,
                    update
                        .cache_key
                        .len()
                        .max(update.href.len())
                        .max(update.name.len()),
                ));
            }
            validate_context_id(&update.target_context_id)?;
            validate_url_text("native WindowProxy update URL", &update.href)?;
            validate_window_name(&update.name)?;
        }
        let mut pending =
            self.pending_window_proxy_updates
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue native WindowProxy updates".into(),
                    reason: "native WindowProxy update queue is unavailable".into(),
                })?;
        *pending = updates.to_vec();
        Ok(())
    }

    fn take_window_proxy_updates(&self) -> Vec<NativeWindowProxyUpdate> {
        self.pending_window_proxy_updates
            .lock()
            .map(|mut updates| std::mem::take(&mut *updates))
            .unwrap_or_default()
    }

    fn apply_window_name_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::SetWindowName { value } = command else {
            return Ok(false);
        };
        validate_window_name(value)?;
        let mut current = self
            .window_name
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "set window name".into(),
                reason: "native window name state lock is unavailable".into(),
            })?;
        *current = value.clone();
        Ok(true)
    }

    fn apply_window_close_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::CloseWindow {
            target,
            target_context_id,
        } = command
        else {
            return Ok(false);
        };
        validate_url_text("window close target", target)?;
        if let Some(target_context_id) = target_context_id {
            validate_context_id(target_context_id)?;
        }
        let mut requests =
            self.window_close_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native window close".into(),
                    reason: "native window close queue is unavailable".into(),
                })?;
        if requests.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native window close requests",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                requests.len().saturating_add(1),
            ));
        }
        requests.push(NativeWindowCloseRequest {
            target: target.clone(),
            target_context_id: target_context_id.clone(),
            source_context_id: String::new(),
        });
        Ok(true)
    }

    fn apply_window_navigation_command(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::NavigateWindow {
            target,
            target_context_id,
            href,
            replace,
        } = command
        else {
            return Ok(false);
        };
        if target.is_empty() && target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "window navigation target",
                "must not be empty without a direct target context",
            ));
        }
        if !target.is_empty() {
            validate_url_text("window navigation target", target)?;
        }
        validate_url_text("window navigation href", href)?;
        if let Some(target_context_id) = target_context_id {
            validate_context_id(target_context_id)?;
        }
        let mut requests =
            self.window_navigation_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native window navigation".into(),
                    reason: "native window navigation queue is unavailable".into(),
                })?;
        if requests.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native window navigation requests",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                requests.len().saturating_add(1),
            ));
        }
        let object_url = if href
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
        {
            self.object_url_resource_in_context(ctx, href)?
        } else {
            None
        };
        requests.push(NativeWindowNavigationRequest {
            target: target.clone(),
            target_context_id: target_context_id.clone(),
            href: href.clone(),
            method: NativeNavigationMethod::Get,
            body: None,
            body_content_type: None,
            replace: *replace,
            object_url,
            source_context_id: String::new(),
        });
        Ok(true)
    }

    fn apply_frame_script_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::FrameScript {
            frame_id,
            source_frame_id,
            command,
        } = command
        else {
            return Ok(false);
        };
        validate_context_id(frame_id)?;
        validate_context_id(source_frame_id)?;
        let current_frame_id = self.frame_id();
        if source_frame_id != &current_frame_id {
            return Err(NativeEngineError::invalid(
                "same-origin frame script source",
                "must identify the current JavaScript frame",
            ));
        }
        if frame_id == source_frame_id {
            return Err(NativeEngineError::invalid(
                "same-origin frame script target",
                "must identify an embedded frame distinct from its caller",
            ));
        }
        if matches!(command.as_ref(), NativeScriptCommand::FrameScript { .. }) {
            return Err(NativeEngineError::invalid(
                "same-origin frame script command",
                "nested frame script commands are not allowed",
            ));
        }
        let encoded =
            serde_json::to_vec(command.as_ref()).map_err(|_| NativeEngineError::Worker {
                operation: "serialize same-origin frame script".into(),
                reason: "same-origin frame script command could not be serialized".into(),
            })?;
        if encoded.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
            return Err(NativeEngineError::limit(
                "same-origin frame script command",
                MAX_NATIVE_SCRIPT_RESULT_BYTES,
                encoded.len(),
            ));
        }
        let mut requests =
            self.frame_script_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue same-origin frame script".into(),
                    reason: "same-origin frame script queue is unavailable".into(),
                })?;
        if requests.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "same-origin frame script requests",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                requests.len().saturating_add(1),
            ));
        }
        requests.push(NativeFrameScriptRequest {
            frame_id: frame_id.clone(),
            source_frame_id: source_frame_id.clone(),
            command: command.clone(),
        });
        Ok(true)
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.clock_origin
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    /// Return the delay until the next page-owned timer or animation frame.
    ///
    /// Timer callbacks live inside the persistent QuickJS realm, so the host
    /// must inspect their bounded schedule before deciding whether an async
    /// evaluation can make progress. The result is relative to this realm's
    /// monotonic clock and never exposes callback objects or page data.
    pub(crate) fn next_timer_delay_ms(&self) -> Result<Option<u64>, NativeEngineError> {
        let now_ms = self.now_ms();
        self.context.with(|ctx| {
            set_native_timer_clock(&ctx, now_ms)?;
            let json: String = ctx.eval(NATIVE_PAGE_TIMER_DELAY_SOURCE).map_err(|error| {
                NativeEngineError::Worker {
                    operation: "inspect native timer queue".into(),
                    reason: format!(
                        "native timer queue could not be inspected: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                }
            })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "native timer queue",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                operation: "decode native timer queue".into(),
                reason: "native timer queue returned an invalid delay".into(),
            })
        })
    }

    /// Return the delay until the next timer in a dedicated worker realm.
    /// Worker queues use a separate namespace from page timers, but share the
    /// same monotonic host clock and bounded inspection contract.
    pub(crate) fn next_worker_timer_delay_ms(&self) -> Result<Option<u64>, NativeEngineError> {
        let now_ms = self.now_ms();
        self.context.with(|ctx| {
            set_native_timer_clock(&ctx, now_ms)?;
            let json: String = ctx
                .eval(NATIVE_WORKER_TIMER_DELAY_SOURCE)
                .map_err(|error| NativeEngineError::Worker {
                    operation: "inspect native Worker timer queue".into(),
                    reason: format!(
                        "native Worker timer queue could not be inspected: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "native Worker timer queue",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                operation: "decode native Worker timer queue".into(),
                reason: "native Worker timer queue returned an invalid delay".into(),
            })
        })
    }

    /// Execute one host event-loop turn for page timers and animation frames.
    ///
    /// The bootstrap timer pump is disabled for this call so each turn runs
    /// the queue exactly once. QuickJS then drains its pending promise jobs,
    /// allowing timer callbacks to settle top-level await or emit the same
    /// bounded DOM/network commands as ordinary script evaluation.
    pub(crate) fn run_timer_turn(
        &self,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let previous = self.timer_pump_enabled();
        self.set_timer_pump_enabled(false);
        let result = self.evaluate(
            "globalThis.__glassRunTimers(performance.now());",
            document,
            document_url,
            origin,
            viewport,
        );
        self.set_timer_pump_enabled(previous);
        result
    }

    /// Deliver one host-owned WebSocket event into the persistent page realm.
    /// The event callback runs on the same serialized QuickJS owner as every
    /// other page task, so DOM mutations and follow-up fetches cannot race the
    /// content snapshot commit.
    pub(crate) fn dispatch_websocket_event(
        &self,
        socket_id: u32,
        event: &serde_json::Value,
        context: NativePageEventContext<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let page_events = NativePageEventBatch {
            csp_violations: context.csp_violations.to_vec(),
            ..NativePageEventBatch::default()
        };
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document: context.document,
                document_url: context.document_url,
                origin: context.origin,
                viewport: context.viewport,
            },
            &page_events,
            Some(NativePageDispatch::WebSocket {
                socket_id,
                payload: event,
            }),
        )
    }

    /// Deliver one host-owned EventSource event into the persistent page
    /// realm. The event callback runs on the same serialized QuickJS owner as
    /// every other page task, so streamed messages cannot race a document
    /// snapshot commit.
    pub(crate) fn dispatch_event_source_event(
        &self,
        source_id: u32,
        event: &serde_json::Value,
        context: NativePageEventContext<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let page_events = NativePageEventBatch {
            csp_violations: context.csp_violations.to_vec(),
            ..NativePageEventBatch::default()
        };
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document: context.document,
                document_url: context.document_url,
                origin: context.origin,
                viewport: context.viewport,
            },
            &page_events,
            Some(NativePageDispatch::EventSource {
                source_id,
                payload: event,
            }),
        )
    }

    /// Deliver one host-owned Fetch response-stream event into the persistent
    /// page realm. Body chunks and terminal state use the same serialized
    /// owner as response continuations and other host events.
    pub(crate) fn dispatch_fetch_stream_event(
        &self,
        stream_id: u32,
        event: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let page_events = NativePageEventBatch::default();
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document,
                document_url,
                origin,
                viewport,
            },
            &page_events,
            Some(NativePageDispatch::FetchStream {
                stream_id,
                payload: event,
            }),
        )
    }

    /// Deliver one HTTP request-body demand to the persistent page realm.
    /// The page-side stream reader answers by emitting exactly one upload
    /// command, keeping the request body pull-driven across the process
    /// boundary.
    pub(crate) fn dispatch_fetch_upload_event(
        &self,
        stream_id: u32,
        event: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document,
                document_url,
                origin,
                viewport,
            },
            &NativePageEventBatch::default(),
            Some(NativePageDispatch::FetchUpload {
                stream_id,
                payload: event,
            }),
        )
    }

    pub(crate) fn dispatch_script_error_event(
        &self,
        node_index: Option<u32>,
        message: &str,
        filename: &str,
        context: NativePageEventContext<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let payload = serde_json::json!({
            "node_index": node_index,
            "message": message,
            "filename": filename,
            "lineno": 0,
            "colno": 0,
        });
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document: context.document,
                document_url: context.document_url,
                origin: context.origin,
                viewport: context.viewport,
            },
            &NativePageEventBatch {
                csp_violations: context.csp_violations.to_vec(),
                ..NativePageEventBatch::default()
            },
            Some(NativePageDispatch::ScriptError { payload: &payload }),
        )
    }

    pub(crate) fn reset_timer_clock(&mut self) {
        self.clock_origin = Instant::now();
    }

    pub(crate) fn set_ready_state(&mut self, ready_state: &str) {
        self.ready_state.clear();
        self.ready_state.push_str(ready_state);
    }

    fn storage_view(&self, document_url: &str, origin: &NativeOrigin) -> NativeWebStorageView {
        let key = storage_key(document_url, origin);
        let Ok(state) = self.storage.lock() else {
            return NativeWebStorageView::default();
        };
        NativeWebStorageView {
            local: state.local.get(&key).cloned().unwrap_or_default(),
            session: state.session.get(&key).cloned().unwrap_or_default(),
        }
    }

    fn apply_storage_command(
        &self,
        command: &NativeScriptCommand,
        document_url: &str,
        origin: &NativeOrigin,
    ) -> Result<Option<NativeStorageEvent>, NativeEngineError> {
        let (scope, entry_key, value, operation) = match command {
            NativeScriptCommand::StorageSet { scope, key, value } => (
                scope.as_str(),
                Some(key.as_str()),
                Some(value.as_str()),
                "set",
            ),
            NativeScriptCommand::StorageRemove { scope, key } => {
                (scope.as_str(), Some(key.as_str()), None, "remove")
            }
            NativeScriptCommand::StorageClear { scope } => (scope.as_str(), None, None, "clear"),
            _ => return Ok(None),
        };
        let mut state = self.storage.lock().map_err(|_| NativeEngineError::Worker {
            operation: "native Web Storage".into(),
            reason: "native Web Storage state lock is unavailable".into(),
        })?;
        let source_context_id = self.storage_context_id.clone();
        let storage = match scope {
            "local" => &mut state.local,
            "session" => &mut state.session,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Web Storage scope",
                    "must be local or session",
                ));
            }
        };
        let origin_key = storage_key(document_url, origin);
        let entries = storage.entry(origin_key.clone()).or_default();
        if let Some(entry_key) = entry_key
            && entry_key.len() > crate::limits::MAX_BACKEND_ID_BYTES
        {
            return Err(NativeEngineError::limit(
                "native Web Storage key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                entry_key.len(),
            ));
        }
        if let Some(value) = value
            && value.len() > crate::limits::MAX_TEXT_BYTES
        {
            return Err(NativeEngineError::limit(
                "native Web Storage value",
                crate::limits::MAX_TEXT_BYTES,
                value.len(),
            ));
        }
        let change = match operation {
            "set" => {
                if entries.len() >= crate::limits::MAX_STORAGE_ENTRIES
                    && entry_key.is_some_and(|key| !entries.contains_key(key))
                {
                    return Err(NativeEngineError::limit(
                        "native Web Storage entries",
                        crate::limits::MAX_STORAGE_ENTRIES,
                        entries.len().saturating_add(1),
                    ));
                }
                let key = entry_key.expect("storage set key");
                let value = value.expect("storage set value");
                let old_value = entries.get(key).cloned();
                if old_value.as_deref() == Some(value) {
                    None
                } else {
                    entries.insert(key.to_owned(), value.to_owned());
                    Some(NativeStorageEvent {
                        source_context_id: source_context_id.clone(),
                        scope: scope.to_owned(),
                        storage_key: origin_key,
                        key: Some(key.to_owned()),
                        old_value,
                        new_value: Some(value.to_owned()),
                        url: document_url.to_owned(),
                    })
                }
            }
            "remove" => {
                let key = entry_key.expect("storage remove key");
                entries.remove(key).map(|old_value| NativeStorageEvent {
                    source_context_id: source_context_id.clone(),
                    scope: scope.to_owned(),
                    storage_key: origin_key,
                    key: Some(key.to_owned()),
                    old_value: Some(old_value),
                    new_value: None,
                    url: document_url.to_owned(),
                })
            }
            "clear" if entries.is_empty() => None,
            "clear" => {
                entries.clear();
                Some(NativeStorageEvent {
                    source_context_id,
                    scope: scope.to_owned(),
                    storage_key: origin_key,
                    key: None,
                    old_value: None,
                    new_value: None,
                    url: document_url.to_owned(),
                })
            }
            _ => unreachable!("storage operation matched above"),
        };
        if let Some(change) = change.as_ref() {
            let mut changes =
                self.storage_changes
                    .lock()
                    .map_err(|_| NativeEngineError::Worker {
                        operation: "record native storage event".into(),
                        reason: "native storage change queue is unavailable".into(),
                    })?;
            if changes.len() >= MAX_NATIVE_STORAGE_EVENTS {
                return Err(NativeEngineError::limit(
                    "native storage changes",
                    MAX_NATIVE_STORAGE_EVENTS,
                    changes.len().saturating_add(1),
                ));
            }
            changes.push(change.clone());
        }
        Ok(change)
    }

    fn apply_cookie_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::CookieSet { value } = command else {
            return Ok(false);
        };
        if value.len() > crate::limits::MAX_TEXT_BYTES {
            return Err(NativeEngineError::limit(
                "native document.cookie value",
                crate::limits::MAX_TEXT_BYTES,
                value.len(),
            ));
        }
        let mut updates = self
            .cookie_updates
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "native document.cookie".into(),
                reason: "native document.cookie update queue is unavailable".into(),
            })?;
        if updates.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native document.cookie updates",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                updates.len().saturating_add(1),
            ));
        }
        updates.push(value.clone());
        Ok(true)
    }

    fn apply_dialog_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::Dialog {
            dialog_type,
            message,
            default_value,
        } = command
        else {
            return Ok(false);
        };
        if !matches!(dialog_type.as_str(), "alert" | "confirm" | "prompt") {
            return Err(NativeEngineError::invalid(
                "native dialog type",
                "must be alert, confirm, or prompt",
            ));
        }
        if message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES {
            return Err(NativeEngineError::limit(
                "native dialog message",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                message.len(),
            ));
        }
        if default_value
            .as_ref()
            .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "native dialog default value",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                default_value.as_ref().map_or(0, String::len),
            ));
        }
        let mut dialogs = self
            .dialog_events
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "record native dialog".into(),
                reason: "native dialog queue is unavailable".into(),
            })?;
        if dialogs.len() >= MAX_NATIVE_DIALOGS {
            return Err(NativeEngineError::limit(
                "native dialogs",
                MAX_NATIVE_DIALOGS,
                dialogs.len().saturating_add(1),
            ));
        }
        dialogs.push(NativeDialog {
            dialog_type: dialog_type.clone(),
            message: message.clone(),
            default_value: default_value.clone(),
        });
        Ok(true)
    }

    fn apply_popup_command(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::OpenWindow {
            href,
            target,
            handle,
        } = command
        else {
            return Ok(false);
        };
        validate_url_text("window.open URL", href)?;
        validate_url_text("window.open target", target)?;
        let mut popups = self
            .popup_events
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "record native popup".into(),
                reason: "native popup queue is unavailable".into(),
            })?;
        if popups.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native popup requests",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                popups.len().saturating_add(1),
            ));
        }
        let object_url = if href
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
        {
            self.object_url_resource_in_context(ctx, href)?
        } else {
            None
        };
        popups.push(NativePopupRequest {
            url: href.clone(),
            target: target.clone(),
            method: NativeNavigationMethod::Get,
            body: None,
            body_content_type: None,
            handle: handle.clone(),
            object_url,
            source_context_id: String::new(),
        });
        Ok(true)
    }

    fn apply_worker_command(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        if !matches!(
            command,
            NativeScriptCommand::SharedWorkerCreate { .. }
                | NativeScriptCommand::WorkerCreate { .. }
                | NativeScriptCommand::WorkerPostMessage { .. }
                | NativeScriptCommand::WorkerTerminate { .. }
                | NativeScriptCommand::WorkerClose { .. }
                | NativeScriptCommand::WorkerScriptError { .. }
        ) {
            return Ok(false);
        }
        match command {
            NativeScriptCommand::SharedWorkerCreate {
                connection_id,
                href,
                name,
                worker_type,
                credentials,
                referrer_policy,
                transfer_port,
                ..
            } => {
                if *connection_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native SharedWorker connection id",
                        "must be positive",
                    ));
                }
                validate_url_text("native SharedWorker URL", href)?;
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
                validate_native_shared_worker_options(worker_type, credentials)?;
                referrer_policy
                    .as_deref()
                    .map(NativeFetchReferrerPolicy::parse)
                    .transpose()?;
                validate_message_port_transfers(std::slice::from_ref(transfer_port))?;
            }
            NativeScriptCommand::WorkerCreate {
                worker_id,
                href,
                worker_type,
                referrer_policy,
            } => {
                if *worker_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native Worker id",
                        "must be positive",
                    ));
                }
                validate_url_text("native Worker URL", href)?;
                let is_module = worker_type.eq_ignore_ascii_case("module");
                if !worker_type.is_empty()
                    && !worker_type.eq_ignore_ascii_case("classic")
                    && !is_module
                {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "native Worker type must be classic or module".into(),
                    });
                }
                referrer_policy
                    .as_deref()
                    .map(NativeFetchReferrerPolicy::parse)
                    .transpose()?;
            }
            NativeScriptCommand::WorkerPostMessage {
                worker_id, data, ..
            } => {
                if *worker_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native Worker id",
                        "must be positive",
                    ));
                }
                let encoded = serde_json::to_vec(data).map_err(|_| NativeEngineError::Worker {
                    operation: "record native Worker message".into(),
                    reason: "Worker message data could not be serialized".into(),
                })?;
                if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Worker message",
                        MAX_NATIVE_POST_MESSAGE_BYTES,
                        encoded.len(),
                    ));
                }
            }
            NativeScriptCommand::WorkerTerminate { worker_id }
            | NativeScriptCommand::WorkerClose { worker_id } => {
                if *worker_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native Worker id",
                        "must be positive",
                    ));
                }
            }
            NativeScriptCommand::WorkerScriptError { .. } => {
                return Err(NativeEngineError::invalid(
                    "native Worker script error",
                    "errors can only be emitted by a dedicated-worker callback turn",
                ));
            }
            _ => unreachable!(),
        }
        let mut commands = self
            .worker_commands
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "record native Worker command".into(),
                reason: "native Worker command queue is unavailable".into(),
            })?;
        if commands.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Worker commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len().saturating_add(1),
            ));
        }
        commands.push(self.attach_object_url_transfers(ctx, command.clone())?);
        Ok(true)
    }

    fn apply_message_port_command(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        if let NativeScriptCommand::MessagePortClose {
            bridge_key,
            worker_id,
        } = command
        {
            validate_url_text("native MessagePort bridge key", bridge_key)?;
            if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
                return Err(NativeEngineError::limit(
                    "native MessagePort bridge key",
                    crate::limits::MAX_BACKEND_ID_BYTES,
                    bridge_key.len(),
                ));
            }
            if worker_id.is_some_and(|worker_id| worker_id == 0) {
                return Err(NativeEngineError::invalid(
                    "native MessagePort worker id",
                    "must be positive when present",
                ));
            }
            let mut commands =
                self.message_port_commands
                    .lock()
                    .map_err(|_| NativeEngineError::Worker {
                        operation: "record native MessagePort command".into(),
                        reason: "native MessagePort command queue is unavailable".into(),
                    })?;
            if commands.len() >= MAX_NATIVE_WORKER_MESSAGES {
                return Err(NativeEngineError::limit(
                    "native MessagePort commands",
                    MAX_NATIVE_WORKER_MESSAGES,
                    commands.len().saturating_add(1),
                ));
            }
            commands.push(command.clone());
            return Ok(true);
        }
        let NativeScriptCommand::MessagePortPostMessage {
            bridge_key,
            data,
            worker_id,
            transfer_ports,
            ..
        } = command
        else {
            return Ok(false);
        };
        validate_url_text("native MessagePort bridge key", bridge_key)?;
        if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                bridge_key.len(),
            ));
        }
        if worker_id.is_some_and(|worker_id| worker_id == 0) {
            return Err(NativeEngineError::invalid(
                "native MessagePort worker id",
                "must be positive when present",
            ));
        }
        validate_message_port_transfers(transfer_ports)?;
        let encoded = serde_json::to_vec(data).map_err(|_| NativeEngineError::Worker {
            operation: "record native MessagePort message".into(),
            reason: "MessagePort data could not be serialized".into(),
        })?;
        if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort message",
                MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        let object_urls = self.object_url_transfers_in_context(ctx, data)?;
        validate_native_object_url_transfers(&object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": data,
                "transfer_ports": transfer_ports,
                "object_urls": &object_urls,
            }),
            "native MessagePort message",
        )?;
        let mut commands =
            self.message_port_commands
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native MessagePort command".into(),
                    reason: "native MessagePort command queue is unavailable".into(),
                })?;
        if commands.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native MessagePort commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len().saturating_add(1),
            ));
        }
        commands.push(NativeScriptCommand::MessagePortPostMessage {
            bridge_key: bridge_key.clone(),
            data: data.clone(),
            worker_id: *worker_id,
            transfer_ports: transfer_ports.clone(),
            object_urls,
        });
        Ok(true)
    }

    pub(super) fn apply_service_worker_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let is_service_worker_command = matches!(
            command,
            NativeScriptCommand::ServiceWorkerRegister { .. }
                | NativeScriptCommand::ServiceWorkerUnregister { .. }
                | NativeScriptCommand::ServiceWorkerUpdate { .. }
                | NativeScriptCommand::ServiceWorkerNavigationPreload { .. }
                | NativeScriptCommand::ServiceWorkerPostMessage { .. }
        );
        if !is_service_worker_command {
            return Ok(false);
        }
        match command {
            NativeScriptCommand::ServiceWorkerRegister {
                request_id,
                script_url,
                scope,
                worker_type,
                referrer_url,
                referrer_policy,
            } => {
                if *request_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native service worker request id",
                        "must be positive",
                    ));
                }
                validate_url_text("native service worker script URL", script_url)?;
                validate_url_text("native service worker scope", scope)?;
                if script_url.len() > MAX_NATIVE_SCRIPT_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker script URL",
                        MAX_NATIVE_SCRIPT_BYTES,
                        script_url.len(),
                    ));
                }
                validate_url_text("native service worker referrer URL", referrer_url)?;
                if referrer_url.len() > MAX_NATIVE_SCRIPT_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker referrer URL",
                        MAX_NATIVE_SCRIPT_BYTES,
                        referrer_url.len(),
                    ));
                }
                if scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker scope",
                        MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                        scope.len(),
                    ));
                }
                if !worker_type.is_empty()
                    && !worker_type.eq_ignore_ascii_case("classic")
                    && !worker_type.eq_ignore_ascii_case("module")
                {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "native service worker type must be classic or module".into(),
                    });
                }
                NativeFetchReferrerPolicy::parse(referrer_policy)?;
            }
            NativeScriptCommand::ServiceWorkerUnregister { request_id, scope } => {
                if *request_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native service worker request id",
                        "must be positive",
                    ));
                }
                validate_url_text("native service worker scope", scope)?;
                if scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker scope",
                        MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                        scope.len(),
                    ));
                }
            }
            NativeScriptCommand::ServiceWorkerUpdate {
                request_id,
                scope,
                referrer_url,
                referrer_policy,
            } => {
                if *request_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native service worker request id",
                        "must be positive",
                    ));
                }
                validate_url_text("native service worker scope", scope)?;
                if scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker scope",
                        MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                        scope.len(),
                    ));
                }
                validate_url_text("native service worker referrer URL", referrer_url)?;
                if referrer_url.len() > MAX_NATIVE_SCRIPT_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker referrer URL",
                        MAX_NATIVE_SCRIPT_BYTES,
                        referrer_url.len(),
                    ));
                }
                NativeFetchReferrerPolicy::parse(referrer_policy)?;
            }
            NativeScriptCommand::ServiceWorkerNavigationPreload {
                request_id,
                scope,
                operation,
                header_value,
            } => {
                if *request_id == 0 {
                    return Err(NativeEngineError::invalid(
                        "native service worker request id",
                        "must be positive",
                    ));
                }
                validate_url_text("native service worker scope", scope)?;
                if scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker scope",
                        MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                        scope.len(),
                    ));
                }
                match (operation.as_str(), header_value.as_deref()) {
                    ("enable" | "disable" | "getState", None) => {}
                    ("setHeaderValue", Some(value)) => {
                        validate_navigation_preload_header_value(value)?;
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native service worker navigation preload operation",
                            "operation and header value do not match the API contract",
                        ));
                    }
                }
            }
            NativeScriptCommand::ServiceWorkerPostMessage {
                scope,
                source_origin,
                data,
                transfer_ports,
            } => {
                validate_url_text("native service worker scope", scope)?;
                if scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker scope",
                        MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                        scope.len(),
                    ));
                }
                validate_url_text("native service worker message origin", source_origin)?;
                if source_origin.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker message origin",
                        MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                        source_origin.len(),
                    ));
                }
                let encoded = serde_json::to_vec(data).map_err(|_| NativeEngineError::Worker {
                    operation: "record native service worker message".into(),
                    reason: "service worker message data could not be serialized".into(),
                })?;
                if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker message",
                        MAX_NATIVE_POST_MESSAGE_BYTES,
                        encoded.len(),
                    ));
                }
                validate_message_port_transfers(transfer_ports)?;
            }
            _ => unreachable!("service worker command was matched above"),
        }
        let mut commands =
            self.service_worker_commands
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue native service worker command".into(),
                    reason: "native service worker command queue was poisoned".into(),
                })?;
        if commands.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native service worker commands",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                commands.len().saturating_add(1),
            ));
        }
        commands.push(command.clone());
        Ok(true)
    }

    fn apply_post_message_command(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::PostMessage {
            target,
            target_origin,
            data,
            target_context_id,
            transfer_ports,
        } = command
        else {
            return Ok(false);
        };
        if target.is_empty() && target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "postMessage target",
                "must not be empty without a direct target context",
            ));
        }
        if !target.is_empty() {
            validate_url_text("postMessage target", target)?;
        }
        validate_url_text("postMessage target origin", target_origin)?;
        if let Some(target_context_id) = target_context_id {
            validate_context_id(target_context_id)?;
        }
        let encoded = serde_json::to_vec(data).map_err(|_| NativeEngineError::Worker {
            operation: "record native postMessage".into(),
            reason: "postMessage data could not be serialized".into(),
        })?;
        if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native postMessage data",
                MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        validate_message_port_transfers(transfer_ports)?;
        let object_urls = self.object_url_transfers_in_context(ctx, data)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": data,
                "transfer_ports": transfer_ports,
                "object_urls": &object_urls,
            }),
            "native postMessage data",
        )?;
        let mut messages =
            self.post_message_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native postMessage".into(),
                    reason: "native postMessage queue is unavailable".into(),
                })?;
        if messages.len() >= crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native postMessage requests",
                crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                messages.len().saturating_add(1),
            ));
        }
        messages.push(NativePostMessageRequest {
            target: target.clone(),
            target_origin: target_origin.clone(),
            data: data.clone(),
            target_context_id: target_context_id.clone(),
            transfer_ports: transfer_ports.clone(),
            object_urls,
            source_context_id: String::new(),
            source_origin: String::new(),
            source_frame_id: self.frame_id(),
        });
        Ok(true)
    }

    pub(crate) fn set_module_sources(&self, sources: BTreeMap<String, String>) {
        if let Ok(mut current) = self.module_sources.lock() {
            *current = sources;
        }
    }

    pub(crate) fn set_module_referrer_policies(
        &self,
        policies: BTreeMap<String, NativeFetchReferrerPolicy>,
    ) {
        if let Ok(mut current) = self.module_referrer_policies.lock() {
            *current = policies;
        }
    }

    pub(crate) fn module_referrer_policies_snapshot(
        &self,
    ) -> Result<BTreeMap<String, NativeFetchReferrerPolicy>, NativeEngineError> {
        self.module_referrer_policies
            .lock()
            .map(|current| current.clone())
            .map_err(|_| NativeEngineError::Worker {
                operation: "read native module referrer policies".into(),
                reason: "native module referrer policy map is unavailable".into(),
            })
    }

    pub(crate) fn extend_module_referrer_policies(
        &self,
        policies: BTreeMap<String, NativeFetchReferrerPolicy>,
    ) -> Result<(), NativeEngineError> {
        self.module_referrer_policies
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "install native module referrer policies".into(),
                reason: "native module referrer policy map is unavailable".into(),
            })?
            .extend(policies);
        Ok(())
    }

    pub(crate) fn module_sources_snapshot(
        &self,
    ) -> Result<ModuleSourcesSnapshot, NativeEngineError> {
        let sources = self
            .module_sources
            .lock()
            .map(|current| current.clone())
            .map_err(|_| NativeEngineError::Worker {
                operation: "read native module sources".into(),
                reason: "native module source map is unavailable".into(),
            })?;
        let base_urls = self
            .module_base_urls
            .lock()
            .map(|current| current.clone())
            .map_err(|_| NativeEngineError::Worker {
                operation: "read native module base URLs".into(),
                reason: "native module base URL map is unavailable".into(),
            })?;
        Ok((sources, base_urls))
    }

    pub(crate) fn extend_module_sources(
        &self,
        sources: BTreeMap<String, String>,
        base_urls: BTreeMap<String, String>,
    ) -> Result<(), NativeEngineError> {
        self.module_sources
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "install native module sources".into(),
                reason: "native module source map is unavailable".into(),
            })?
            .extend(sources);
        self.module_base_urls
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "install native module base URLs".into(),
                reason: "native module base URL map is unavailable".into(),
            })?
            .extend(base_urls);
        Ok(())
    }

    fn set_dynamic_module_import_mode(
        &self,
        mode: NativeModuleDynamicImportMode,
    ) -> Result<(), NativeEngineError> {
        *self
            .dynamic_import_mode
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "configure native dynamic module imports".into(),
                reason: "native module loader configuration is unavailable".into(),
            })? = mode;
        Ok(())
    }

    fn set_dynamic_module_imports_enabled(&self, enabled: bool) -> Result<(), NativeEngineError> {
        self.set_dynamic_module_import_mode(if enabled {
            NativeModuleDynamicImportMode::Resolve
        } else {
            NativeModuleDynamicImportMode::Preserve
        })
    }

    pub(crate) fn register_dynamic_import_referrer(
        &self,
        referrer: &str,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("dynamic module referrer", referrer)?;
        let mut referrers =
            self.dynamic_import_referrers
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "register dynamic module referrer".into(),
                    reason: "dynamic module referrer set is unavailable".into(),
                })?;
        if !referrers.contains(referrer) && referrers.len() >= MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "dynamic module referrers",
                MAX_NATIVE_MODULE_IMPORTS,
                referrers.len().saturating_add(1),
            ));
        }
        referrers.insert(referrer.to_owned());
        Ok(())
    }

    pub(crate) fn is_dynamic_import_referrer_allowed(
        &self,
        referrer: &str,
    ) -> Result<bool, NativeEngineError> {
        if self
            .dynamic_import_referrers
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "validate dynamic module referrer".into(),
                reason: "dynamic module referrer set is unavailable".into(),
            })?
            .contains(referrer)
        {
            return Ok(true);
        }
        Ok(self
            .module_base_urls
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "validate dynamic module referrer".into(),
                reason: "native module base URL map is unavailable".into(),
            })?
            .values()
            .any(|base_url| base_url == referrer))
    }

    pub(crate) fn register_dynamic_module_alias(
        &self,
        request_id: u32,
        target: &str,
    ) -> Result<String, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "dynamic module request id",
                "must be positive",
            ));
        }
        let alias = format!("glass-internal-dynamic-module:{request_id}");
        let mut aliases =
            self.dynamic_module_aliases
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "register dynamic module alias".into(),
                    reason: "dynamic module alias map is unavailable".into(),
                })?;
        if aliases.len() >= MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "pending dynamic module aliases",
                MAX_NATIVE_MODULE_IMPORTS,
                aliases.len().saturating_add(1),
            ));
        }
        if aliases.insert(alias.clone(), target.to_owned()).is_some() {
            return Err(NativeEngineError::invalid(
                "dynamic module request id",
                "must not be reused while an import is pending",
            ));
        }
        Ok(alias)
    }

    pub(crate) fn discard_dynamic_module_alias(&self, alias: &str) {
        if let Ok(mut aliases) = self.dynamic_module_aliases.lock() {
            aliases.remove(alias);
        }
    }

    pub(crate) fn set_module_base_urls(&self, base_urls: BTreeMap<String, String>) {
        if let Ok(mut current) = self.module_base_urls.lock() {
            *current = base_urls;
        }
    }

    pub(crate) fn set_module_import_map(&self, import_map: NativeModuleImportMap) {
        if let Ok(mut current) = self.module_import_map.lock() {
            *current = import_map;
        }
    }

    pub(crate) fn module_import_map(&self) -> Result<NativeModuleImportMap, NativeEngineError> {
        self.module_import_map
            .lock()
            .map(|current| current.clone())
            .map_err(|_| NativeEngineError::Worker {
                operation: "read page module import map".into(),
                reason: "page module import map is unavailable".into(),
            })
    }
}
