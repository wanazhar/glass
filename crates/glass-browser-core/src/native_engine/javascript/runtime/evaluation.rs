use super::*;

impl NativeJavaScriptRuntime {
    pub(crate) fn evaluate(
        &self,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_classic_script(
            source,
            document_url,
            document,
            document_url,
            origin,
            viewport,
        )
    }

    pub(crate) fn evaluate_classic_script(
        &self,
        source: &str,
        script_base_url: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(script_base_url)?;
        let referrer =
            serde_json::to_string(script_base_url).map_err(|_| NativeEngineError::Worker {
                operation: "prepare dynamic module referrer".into(),
                reason: "dynamic module referrer could not be encoded".into(),
            })?;
        let source = rewrite_runtime_dynamic_module_imports(source, &referrer)?;
        self.evaluate_with_page_events(
            &source,
            document,
            document_url,
            origin,
            viewport,
            &NativePageEventBatch::default(),
        )
    }

    pub(crate) fn evaluate_with_host_events(
        &self,
        events: &[NativeHostEvent],
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let page_events = NativePageEventBatch {
            host_events: events.to_vec(),
            ..NativePageEventBatch::default()
        };
        self.evaluate_with_page_events(
            NATIVE_HOST_EVENT_RESULT_SOURCE,
            document,
            document_url,
            origin,
            viewport,
            &page_events,
        )
    }

    pub(crate) fn evaluate_with_page_events(
        &self,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
        page_events: &NativePageEventBatch,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_with_page_events_and_dispatch(
            source,
            NativePageEvaluationContext {
                document,
                document_url,
                origin,
                viewport,
            },
            page_events,
            None,
        )
    }

    pub(super) fn evaluate_with_page_events_and_dispatch(
        &self,
        source: &str,
        context: NativePageEvaluationContext<'_>,
        page_events: &NativePageEventBatch,
        dispatch: Option<NativePageDispatch<'_>>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativePageEvaluationContext {
            document,
            document_url,
            origin,
            viewport,
        } = context;
        self.set_dialog_url(document_url)?;
        let document_creation_url = self.document_creation_url_for(document, document_url)?;
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "script source",
                "must not be empty",
            ));
        }
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "script source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        let storage_events = self.take_storage_events();
        let proxy_updates = self.take_window_proxy_updates();
        let window_name = self.window_name();
        let opener_window_name = self.opener_window_name();
        let frame_id = self.frame_id();
        let scroll_offset = self.scroll_offset();
        let nested_scroll_offsets = self.nested_scroll_offsets();
        let history_state = self.history_state();
        let history_length = self.history_length();
        let include_layout = !is_ready_state_comparison(source);
        let bootstrap_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "javascript_document_bootstrap",
            )
        });
        let bootstrap = document_bootstrap(
            document,
            document_url,
            &document_creation_url,
            &frame_id,
            &window_name,
            self.opener_context_id(),
            opener_window_name,
            &self.opener_url,
            origin,
            viewport,
            scroll_offset,
            &nested_scroll_offsets,
            &history_state,
            history_length,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
            &self.indexed_db_state(),
            &storage_events,
            &self.cookie_state(),
            &self.frame_script_bindings(),
            self.frame_script_context().as_ref(),
            &self.service_worker_registrations(),
            self.timer_pump_enabled(),
            include_layout,
            &self.environment(),
        )?;
        if let Some(stage) = bootstrap_stage {
            stage.finish(Some(
                bootstrap
                    .source
                    .len()
                    .saturating_add(bootstrap.host_state_json.len()),
            ));
        }
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let evaluation_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "javascript_context_evaluation",
            )
        });
        let result = self.context.with(|ctx| {
            install_native_crypto_sources(ctx.clone())?;
            install_native_sync_xhr_source(
                ctx.clone(),
                Arc::clone(&self.sync_xhr_loader),
                Arc::clone(&self.sync_xhr_loader_used),
                Arc::clone(&self.cookie),
                Arc::clone(&self.cookie_updates),
                Arc::clone(&self.parent_brokered_cookie_updates),
            )?;
            install_native_local_font_source(ctx.clone())?;
            install_native_url_source(ctx.clone())?;
            install_native_inline_script_policy(
                ctx.clone(),
                Arc::clone(&self.inline_script_policy),
                Arc::clone(&self.processed_inline_csp_meta_nodes),
                document_url,
            )?;
            install_native_dialog_host_call(
                ctx.clone(),
                Arc::clone(&self.dialog_handler),
                Arc::clone(&self.dialog_url),
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "install native dialog host callback".into(),
                reason: "native JavaScript dialog host callback could not be installed".into(),
            })?;
            let host_view_bytes = bootstrap
                .source
                .len()
                .saturating_add(bootstrap.host_state_json.len());
            let host_view_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
                crate::native_engine::content_process::ContentLoadStage::begin(
                    0,
                    "javascript_host_view_eval",
                )
            });
            if let Ok(mut current) = self.deadline.lock() {
                *current = Some(Instant::now() + NATIVE_HOST_VIEW_TIMEOUT);
            }
            let host_view_result = ctx
                .globals()
                .set("__glassHostViewJson", bootstrap.host_state_json.as_str())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "install JavaScript host snapshot".into(),
                    reason: format!(
                        "native JavaScript host snapshot could not be installed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })
                .and_then(|()| {
                    ctx.eval::<(), _>(bootstrap.source.as_str())
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "install JavaScript host view".into(),
                            reason: format!(
                                "native JavaScript host view could not be installed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })
                });
            if let Some(stage) = host_view_stage {
                if host_view_result.is_ok() {
                    stage.finish(Some(host_view_bytes));
                } else {
                    stage.fail(Some(host_view_bytes));
                }
            }
            if let Ok(mut current) = self.deadline.lock() {
                *current = Some(Instant::now() + NATIVE_SCRIPT_TIMEOUT);
            }
            host_view_result?;
            dispatch_window_proxy_updates(&ctx, &proxy_updates)?;
            dispatch_page_event_batch(ctx.clone(), page_events)?;
            if let Some(dispatch) = dispatch {
                dispatch_page_payload(ctx.clone(), dispatch)?;
            }
            let mut top_level_await_pending = false;
            let (value, async_evaluation): (Value, bool) = match ctx.eval::<Value, _>(source) {
                Ok(value) => (value, false),
                Err(_) if contains_await_token(source) => {
                    let promise = ctx.eval_promise(source).map_err(|error| {
                        NativeEngineError::Worker {
                            operation: "evaluate JavaScript".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        }
                    })?;
                    ctx.eval::<(), _>(
                        "globalThis.__glassTopLevelAwaitState = { state: 'pending' };",
                    )
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "prepare JavaScript evaluation promise".into(),
                        reason: format!(
                            "JavaScript evaluation failed: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                    let on_fulfilled: Function = ctx
                        .eval("value => { globalThis.__glassTopLevelAwaitState = { state: 'fulfilled', value: value === undefined ? null : value }; }")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "prepare JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    let on_rejected: Function = ctx
                        .eval("error => { globalThis.__glassTopLevelAwaitState = { state: 'rejected', error: String(error) }; }")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "prepare JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    promise
                        .then()
                        .and_then(|then| {
                            then.call::<_, ()>((
                                This(promise.clone()),
                                on_fulfilled,
                                on_rejected,
                            ))
                        })
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "prepare JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    // A local promise (for example, a Blob object-URL fetch)
                    // has no host event to wake the outer evaluator. Drain
                    // the bounded QuickJS job queue once before classifying
                    // it as genuinely host-pending; network-backed promises
                    // remain pending and continue through the host loop.
                    for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                        if !ctx.execute_pending_job() {
                            break;
                        }
                    }
                    match promise.finish::<Value>() {
                        Ok(value) => (value, true),
                        Err(Error::WouldBlock) => {
                            top_level_await_pending = true;
                            let value =
                                ctx.eval::<Value, _>("undefined").map_err(|error| {
                                    NativeEngineError::Worker {
                                        operation: "evaluate JavaScript".into(),
                                        reason: format!(
                                            "JavaScript evaluation failed: {}",
                                            CaughtError::from_error(&ctx, error)
                                        ),
                                    }
                                })?;
                            (value, true)
                        }
                        Err(error) => {
                            return Err(NativeEngineError::Worker {
                                operation: "evaluate JavaScript".into(),
                                reason: format!(
                                    "JavaScript evaluation failed: {}",
                                    CaughtError::from_error(&ctx, error)
                                ),
                            });
                        }
                    }
                }
                Err(error) => {
                    return Err(NativeEngineError::Worker {
                        operation: "evaluate JavaScript".into(),
                        reason: format!(
                            "JavaScript evaluation failed: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    });
                }
            };
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueResizeObserverChanges === 'function') globalThis.__glassQueueResizeObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native resize observers".into(),
                reason: "native resize observers could not be scheduled".into(),
            })?;
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueIntersectionObserverChanges === 'function') globalThis.__glassQueueIntersectionObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native intersection observers".into(),
                reason: "native intersection observers could not be scheduled".into(),
            })?;
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            for (event_type, cancelable, reasons) in [
                (
                    "unhandledrejection",
                    true,
                    self.take_unhandled_promise_rejections(),
                ),
                (
                    "rejectionhandled",
                    false,
                    self.take_handled_promise_rejections(),
                ),
            ] {
                dispatch_promise_rejections(&ctx, event_type, &reasons, cancelable)?;
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut document_commands = Vec::with_capacity(commands.len());
            for command in commands {
                if self
                    .apply_storage_command(&command, document_url, origin)?
                    .is_some()
                {
                    continue;
                }
                if self.apply_cookie_command(&command)? {
                    continue;
                }
                if self.apply_dialog_command(&command)? {
                    continue;
                }
                if self.apply_popup_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_window_name_command(&command)? {
                    continue;
                }
                if self.apply_window_close_command(&command)? {
                    continue;
                }
                if self.apply_window_navigation_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_frame_script_command(&command)? {
                    continue;
                }
                if self.apply_post_message_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_message_port_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_service_worker_command(&command)? {
                    continue;
                }
                if self.apply_worker_command(&ctx, &command)? {
                    continue;
                }
                document_commands.push(command);
            }
            let commands = document_commands;
            let indexed_db_state = read_indexed_db_state(ctx.clone())?;
            self.set_indexed_db_state(indexed_db_state);
            let json = ctx
                .json_stringify(value)
                .map_err(|error| NativeEngineError::Worker {
                    operation: "serialize JavaScript result".into(),
                    reason: format!(
                        "JavaScript result could not be serialized: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            let Some(json) = json else {
                return Ok(NativeScriptEvaluation {
                    value: serde_json::Value::Null,
                    commands,
                    top_level_await_pending,
                    worker_script_error: None,
                });
            };
            let json = json.to_string().map_err(|_| NativeEngineError::Worker {
                operation: "serialize JavaScript result".into(),
                reason: "JavaScript result could not be converted to UTF-8".into(),
            })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "script result",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            let mut result: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                    operation: "decode JavaScript result".into(),
                    reason: "JavaScript result was not valid JSON".into(),
                })?;
            if async_evaluation
                && let Some(object) = result.as_object_mut()
                && object.len() == 1
                && let Some(value) = object.remove("value")
            {
                result = value;
            }
            Ok(NativeScriptEvaluation {
                value: result,
                commands,
                top_level_await_pending,
                worker_script_error: None,
            })
        });
        let heap_used_bytes = self.runtime.memory_usage().memory_used_size;
        if heap_used_bytes > (NATIVE_SCRIPT_MEMORY_BYTES / 2) as i64 {
            let gc_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
                crate::native_engine::content_process::ContentLoadStage::begin(
                    0,
                    "javascript_heap_gc",
                )
            });
            self.runtime.run_gc();
            if let Some(stage) = gc_stage {
                stage.finish(Some(
                    usize::try_from(self.runtime.memory_usage().memory_used_size)
                        .unwrap_or(usize::MAX),
                ));
            }
        }
        if std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").is_some() {
            let memory_stage = crate::native_engine::content_process::ContentLoadStage::begin(
                0,
                "javascript_heap_usage_bytes",
            );
            let used_bytes = self.runtime.memory_usage().memory_used_size;
            memory_stage.finish(Some(usize::try_from(used_bytes).unwrap_or(usize::MAX)));
        }
        if let Some(stage) = evaluation_stage {
            stage.finish(None);
        }
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    /// Evaluate one classic dedicated-worker turn in an isolated QuickJS
    /// realm. Workers receive no document or page host objects; only the
    /// bounded worker-global message surface is installed.
    pub(crate) fn evaluate_worker(
        &self,
        worker_id: u32,
        worker_url: &str,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(worker_url)?;
        let referrer =
            serde_json::to_string(worker_url).map_err(|_| NativeEngineError::Worker {
                operation: "prepare Worker dynamic module referrer".into(),
                reason: "Worker dynamic module referrer could not be encoded".into(),
            })?;
        let source = rewrite_runtime_dynamic_module_imports(source, &referrer)?;
        self.evaluate_worker_source(worker_id, worker_url, None, &source, import_script_counts)
    }

    /// Evaluate a classic Worker graph whose computed imports were already
    /// rewritten per source resource before the scripts were concatenated.
    pub(crate) fn evaluate_worker_prepared(
        &self,
        worker_id: u32,
        worker_url: &str,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(worker_url)?;
        let bootstrap = worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            import_script_counts,
            false,
            self.worker_global_referrer_policy,
            WorkerBootstrapCapabilities {
                report_message_callback_errors: true,
                forward_message_callback_errors: true,
                capture_connect_callback_errors: false,
            },
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event_impl(
            worker_id, worker_url, None, source, bootstrap, false, false, None, true, false,
        )
    }

    /// Evaluate one module dedicated-worker turn using the prefetched module
    /// graph installed in the runtime's native module loader.
    pub(crate) fn evaluate_worker_module(
        &self,
        request: NativeWorkerModuleEvaluation<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativeWorkerModuleEvaluation {
            worker_id,
            worker_url,
            module_name,
            module_base_url,
            source,
            module_sources,
            module_base_urls,
            module_referrer_policies,
            referrer_policy,
            capture_startup_runtime_errors,
        } = request;
        if module_name.is_empty() {
            return Err(NativeEngineError::invalid(
                "worker module name",
                "must not be empty",
            ));
        }
        self.set_module_sources(module_sources.clone());
        self.set_module_referrer_policies(module_referrer_policies.clone());
        let mut module_base_urls = module_base_urls.clone();
        module_base_urls.insert(module_name.to_owned(), module_base_url.to_owned());
        self.set_module_base_urls(module_base_urls);
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(module_base_url)?;
        let referrer =
            serde_json::to_string(module_base_url).map_err(|_| NativeEngineError::Worker {
                operation: "prepare Worker module referrer".into(),
                reason: "Worker module referrer could not be encoded".into(),
            })?;
        let referrer_policy = native_module_referrer_policy_expression(referrer_policy);
        let source = rewrite_runtime_dynamic_module_imports_with_policy(
            source,
            &referrer,
            &referrer_policy,
        )?;
        let bootstrap = worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            true,
            self.worker_global_referrer_policy,
            WorkerBootstrapCapabilities {
                report_message_callback_errors: true,
                forward_message_callback_errors: true,
                capture_connect_callback_errors: false,
            },
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event_impl(
            worker_id,
            worker_url,
            Some(module_name),
            &source,
            bootstrap,
            false,
            false,
            None,
            false,
            capture_startup_runtime_errors,
        )
    }

    /// Evaluate one classic SharedWorker turn using the shared-worker global
    /// bootstrap. The runtime itself is shared by all page connections; each
    /// connection enters through its transferred MessagePort.
    pub(crate) fn evaluate_shared_worker(
        &self,
        worker_id: u32,
        worker_url: &str,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(worker_url)?;
        let referrer =
            serde_json::to_string(worker_url).map_err(|_| NativeEngineError::Worker {
                operation: "prepare SharedWorker dynamic module referrer".into(),
                reason: "SharedWorker dynamic module referrer could not be encoded".into(),
            })?;
        let source = rewrite_runtime_dynamic_module_imports(source, &referrer)?;
        self.evaluate_shared_worker_source(
            worker_id,
            worker_url,
            None,
            &source,
            import_script_counts,
        )
    }

    /// Evaluate a classic SharedWorker graph whose computed imports were
    /// already rewritten per source resource before the scripts were
    /// concatenated.
    pub(crate) fn evaluate_shared_worker_prepared(
        &self,
        worker_id: u32,
        worker_url: &str,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(worker_url)?;
        self.evaluate_shared_worker_source(
            worker_id,
            worker_url,
            None,
            source,
            import_script_counts,
        )
    }

    /// Evaluate one module SharedWorker turn using the prefetched module
    /// graph installed in the shared runtime loader.
    pub(crate) fn evaluate_shared_worker_module(
        &self,
        request: NativeWorkerModuleEvaluation<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativeWorkerModuleEvaluation {
            worker_id,
            worker_url,
            module_name,
            module_base_url,
            source,
            module_sources,
            module_base_urls,
            module_referrer_policies,
            referrer_policy,
            ..
        } = request;
        if module_name.is_empty() {
            return Err(NativeEngineError::invalid(
                "SharedWorker module name",
                "must not be empty",
            ));
        }
        self.set_module_sources(module_sources.clone());
        self.set_module_referrer_policies(module_referrer_policies.clone());
        let mut module_base_urls = module_base_urls.clone();
        module_base_urls.insert(module_name.to_owned(), module_base_url.to_owned());
        self.set_module_base_urls(module_base_urls);
        self.set_dynamic_module_imports_enabled(true)?;
        self.register_dynamic_import_referrer(module_base_url)?;
        let referrer =
            serde_json::to_string(module_base_url).map_err(|_| NativeEngineError::Worker {
                operation: "prepare SharedWorker module referrer".into(),
                reason: "SharedWorker module referrer could not be encoded".into(),
            })?;
        let referrer_policy = native_module_referrer_policy_expression(referrer_policy);
        let source = rewrite_runtime_dynamic_module_imports_with_policy(
            source,
            &referrer,
            &referrer_policy,
        )?;
        self.evaluate_shared_worker_source(
            worker_id,
            worker_url,
            Some(module_name),
            &source,
            &BTreeMap::new(),
        )
    }

    fn evaluate_shared_worker_source(
        &self,
        worker_id: u32,
        worker_url: &str,
        module_name: Option<&str>,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let bootstrap = shared_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            import_script_counts,
            module_name.is_some(),
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name,
            },
            source,
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: false,
                await_promise: false,
            },
        )
    }

    pub(crate) fn evaluate_shared_worker_connect(
        &self,
        worker_id: u32,
        worker_url: &str,
        transfer_port: &NativeMessagePortTransfer,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        validate_message_port_transfers(std::slice::from_ref(transfer_port))?;
        let bootstrap = shared_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: false,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::SharedWorkerConnect {
                transfer_ports: std::slice::from_ref(transfer_port),
            }),
        )
    }

    fn evaluate_worker_source(
        &self,
        worker_id: u32,
        worker_url: &str,
        module_name: Option<&str>,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let bootstrap = worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            import_script_counts,
            module_name.is_some(),
            self.worker_global_referrer_policy,
            WorkerBootstrapCapabilities {
                report_message_callback_errors: true,
                forward_message_callback_errors: true,
                capture_connect_callback_errors: false,
            },
        )?;
        self.evaluate_worker_source_with_bootstrap(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name,
            },
            source,
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: false,
                await_promise: false,
            },
        )
    }

    pub(in crate::native_engine::javascript) fn evaluate_worker_source_with_bootstrap(
        &self,
        identity: NativeWorkerSourceIdentity<'_>,
        source: &str,
        bootstrap: String,
        mode: NativeWorkerExecutionMode,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativeWorkerSourceIdentity {
            worker_id,
            worker_url,
            module_name,
        } = identity;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name,
            },
            source,
            bootstrap,
            mode,
            None,
        )
    }

    pub(in crate::native_engine::javascript) fn evaluate_worker_source_with_bootstrap_and_event(
        &self,
        identity: NativeWorkerSourceIdentity<'_>,
        source: &str,
        bootstrap: String,
        mode: NativeWorkerExecutionMode,
        dispatch: Option<NativeWorkerDispatch<'_>>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativeWorkerSourceIdentity {
            worker_id,
            worker_url,
            module_name,
        } = identity;
        self.evaluate_worker_source_with_bootstrap_and_event_impl(
            worker_id,
            worker_url,
            module_name,
            source,
            bootstrap,
            mode.service_worker,
            mode.await_promise,
            dispatch,
            false,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_worker_source_with_bootstrap_and_event_impl(
        &self,
        worker_id: u32,
        worker_url: &str,
        module_name: Option<&str>,
        source: &str,
        bootstrap: String,
        service_worker: bool,
        await_promise: bool,
        dispatch: Option<NativeWorkerDispatch<'_>>,
        capture_classic_worker_error: bool,
        capture_module_worker_error: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "native Worker id",
                "must be positive",
            ));
        }
        validate_url_text("native Worker URL", worker_url)?;
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "worker script source",
                "must not be empty",
            ));
        }
        let transformed_module_source = module_name
            .map(|_| rewrite_static_json_module_specifiers(source))
            .transpose()?;
        let source = transformed_module_source.as_deref().unwrap_or(source);
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "worker script source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let mut worker_script_error = None;
        let result = self.context.with(|ctx| {
            install_native_crypto_sources(ctx.clone())?;
            install_native_sync_xhr_source(
                ctx.clone(),
                Arc::clone(&self.sync_xhr_loader),
                Arc::clone(&self.sync_xhr_loader_used),
                Arc::clone(&self.cookie),
                Arc::clone(&self.cookie_updates),
                Arc::clone(&self.parent_brokered_cookie_updates),
            )?;
            install_native_url_source(ctx.clone())?;
            ctx.eval::<(), _>(bootstrap.as_str())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "install native Worker host view".into(),
                    reason: format!(
                        "native Worker host view could not be installed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            let service_worker_message_dispatch = if service_worker {
                let dispatcher: Function = ctx
                    .globals()
                    .get("__glassDispatchServiceWorkerMessage")
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "prepare native Service Worker message dispatcher".into(),
                        reason: format!(
                            "native Service Worker message dispatcher was unavailable: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                ctx.eval::<(), _>("delete globalThis.__glassDispatchServiceWorkerMessage;")
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "protect native Service Worker message dispatcher".into(),
                        reason: format!(
                            "native Service Worker message dispatcher could not be hidden: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                Some(dispatcher)
            } else {
                None
            };
            if service_worker {
                dispatch_service_worker_clients(&ctx, &self.service_worker_clients())?;
            }
            let await_dispatch = if let Some(dispatch) = dispatch {
                dispatch_worker_event(ctx.clone(), dispatch, service_worker_message_dispatch)?
            } else {
                false
            };
            let evaluation_source = if await_dispatch {
                "await globalThis.__glassPendingNativeWorkerPromise.then(value => { globalThis.__glassPendingNativeWorkerPromise = undefined; return value; }, error => { globalThis.__glassPendingNativeWorkerPromise = undefined; throw error; });"
            } else {
                source
            };
            let mut top_level_await_pending = false;
            let value: Value = if await_promise {
                let promise = ctx.eval_promise(evaluation_source).map_err(|error| {
                    NativeEngineError::Worker {
                        operation: "evaluate native service worker promise".into(),
                        reason: format!(
                            "native service worker promise could not be created: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    }
                })?;
                ctx.eval::<(), _>(
                    "globalThis.__glassTopLevelAwaitState = { state: 'pending' };",
                )
                .map_err(|error| NativeEngineError::Worker {
                    operation: "prepare native service worker promise".into(),
                    reason: format!(
                        "native service worker promise could not be prepared: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
                let on_fulfilled: Function = ctx
                    .eval("value => { globalThis.__glassTopLevelAwaitState = { state: 'fulfilled', value: value === undefined ? null : value }; }")
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "prepare native service worker promise".into(),
                        reason: format!(
                            "native service worker promise callback could not be created: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                let on_rejected: Function = ctx
                    .eval("error => { globalThis.__glassTopLevelAwaitState = { state: 'rejected', error: String(error) }; }")
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "prepare native service worker promise".into(),
                        reason: format!(
                            "native service worker promise callback could not be created: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                promise
                    .then()
                    .and_then(|then| {
                        then.call::<_, ()>((
                            This(promise.clone()),
                            on_fulfilled,
                            on_rejected,
                        ))
                    })
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "prepare native service worker promise".into(),
                        reason: format!(
                            "native service worker promise callback could not be attached: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                match promise.finish::<Value>() {
                    Ok(value) => value,
                    Err(Error::WouldBlock) => {
                        top_level_await_pending = true;
                        ctx.eval::<Value, _>("undefined").map_err(|error| {
                            NativeEngineError::Worker {
                                operation: "serialize native service worker promise".into(),
                                reason: format!(
                                    "native service worker promise result could not be created: {}",
                                    CaughtError::from_error(&ctx, error)
                                ),
                            }
                        })?
                    }
                    Err(error) => {
                        return Err(NativeEngineError::Worker {
                            operation: "evaluate native service worker promise".into(),
                            reason: format!(
                                "native service worker promise failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        });
                    }
                }
            } else if let Some(module_name) = module_name {
                let promise = if capture_module_worker_error {
                    let module = Module::declare(ctx.clone(), module_name, source).map_err(
                        |error| NativeEngineError::Worker {
                            operation: "evaluate native Worker module".into(),
                            reason: format!(
                                "native Worker module evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        },
                    )?;
                    let (_module, promise) = module.eval().map_err(|error| {
                        NativeEngineError::Worker {
                            operation: "evaluate native Worker module".into(),
                            reason: format!(
                                "native Worker module evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        }
                    })?;
                    promise
                } else {
                    Module::evaluate(ctx.clone(), module_name, source).map_err(|error| {
                        NativeEngineError::Worker {
                            operation: "evaluate native Worker module".into(),
                            reason: format!(
                                "native Worker module evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        }
                    })?
                };
                match promise.finish::<()>() {
                    Ok(()) => {}
                    Err(Error::WouldBlock) => {
                        retain_top_level_await_promise(&ctx, &promise)?;
                        top_level_await_pending = true;
                    }
                    Err(Error::Exception) if capture_module_worker_error => {
                        worker_script_error =
                            report_worker_script_exception(&ctx, ctx.catch())?;
                    }
                    Err(error) => {
                        return Err(NativeEngineError::Worker {
                            operation: "evaluate native Worker module".into(),
                            reason: format!(
                                "native Worker module evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        });
                    }
                }
                ctx.eval("undefined")
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "serialize native Worker result".into(),
                        reason: format!(
                            "native Worker result could not be created: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?
            } else if capture_classic_worker_error && !service_worker {
                match execute_classic_worker_script(ctx.clone(), source, worker_url)? {
                    NativeClassicWorkerScriptExecution::Completed => {
                        ctx.eval::<Value, _>("undefined").map_err(|error| {
                            NativeEngineError::Worker {
                                operation: "serialize native Worker result".into(),
                                reason: format!(
                                    "native Worker result could not be created: {}",
                                    CaughtError::from_error(&ctx, error)
                                ),
                            }
                        })?
                    }
                    NativeClassicWorkerScriptExecution::RuntimeError(exception) => {
                        worker_script_error =
                            report_worker_script_exception(&ctx, exception)?;
                        ctx.eval::<Value, _>("undefined").map_err(|error| {
                            NativeEngineError::Worker {
                                operation: "serialize native Worker result".into(),
                                reason: format!(
                                    "native Worker result could not be created: {}",
                                    CaughtError::from_error(&ctx, error)
                                ),
                            }
                        })?
                    }
                }
            } else {
                ctx.eval(source)
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "evaluate native Worker script".into(),
                        reason: format!(
                            "native Worker script evaluation failed: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?
            };
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut worker_commands = Vec::with_capacity(commands.len());
            for command in commands {
                let command = self.attach_object_url_transfers(&ctx, command)?;
                if service_worker
                    && (is_service_worker_cache_command(&command)
                        || is_service_worker_lifecycle_command(&command)
                        || is_service_worker_open_window_command(&command))
                {
                    if is_service_worker_cache_command(&command) {
                        validate_service_worker_cache_command(&command)?;
                    } else if is_service_worker_lifecycle_command(&command) {
                        validate_service_worker_lifecycle_command(&command)?;
                    } else {
                        validate_service_worker_open_window_command(&command)?;
                    }
                    worker_commands.push(command);
                    continue;
                }
                if matches!(
                    &command,
                    NativeScriptCommand::MessagePortPostMessage {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                        | NativeScriptCommand::MessagePortClose {
                            worker_id: Some(command_worker_id),
                            ..
                        } if *command_worker_id == worker_id
                )
                    && self.apply_message_port_command(&ctx, &command)?
                {
                    continue;
                }
                let valid = match &command {
                    NativeScriptCommand::WorkerPostMessage { .. }
                    | NativeScriptCommand::WorkerClose { .. } => !service_worker,
                    NativeScriptCommand::WorkerScriptError {
                        worker_id: command_worker_id,
                        message,
                    } => {
                        if *command_worker_id == worker_id
                            && !service_worker
                            && message.len() > MAX_NATIVE_WORKER_SCRIPT_ERROR_BYTES
                        {
                            return Err(NativeEngineError::limit(
                                "native Worker script error message",
                                MAX_NATIVE_WORKER_SCRIPT_ERROR_BYTES,
                                message.len(),
                            ));
                        }
                        *command_worker_id == worker_id && !service_worker
                    }
                    NativeScriptCommand::Fetch {
                        worker_id: Some(command_worker_id),
                        ..
                    } => *command_worker_id == worker_id,
                    NativeScriptCommand::WebSocketOpen {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::WebSocketSend {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::WebSocketClose {
                        worker_id: Some(command_worker_id),
                        ..
                    } => *command_worker_id == worker_id,
                    NativeScriptCommand::EventSourceOpen {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::EventSourceClose {
                        worker_id: Some(command_worker_id),
                        ..
                    } => *command_worker_id == worker_id,
                    NativeScriptCommand::FetchStreamRead {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::FetchStreamCancel {
                        worker_id: Some(command_worker_id),
                        ..
                    } => *command_worker_id == worker_id,
                    NativeScriptCommand::FetchUploadChunk {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::FetchUploadEnd {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::FetchUploadError {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::FetchUploadCancel {
                        worker_id: Some(command_worker_id),
                        ..
                    } => *command_worker_id == worker_id,
                    NativeScriptCommand::MessagePortPostMessage {
                        worker_id: Some(command_worker_id),
                        ..
                    }
                    | NativeScriptCommand::MessagePortClose {
                        worker_id: Some(command_worker_id),
                        ..
                    } => *command_worker_id == worker_id,
                    NativeScriptCommand::ServiceWorkerClientPostMessage {
                        worker_id: command_worker_id,
                        ..
                    } => service_worker && *command_worker_id == worker_id,
                    NativeScriptCommand::ServiceWorkerSkipWaiting {
                        worker_id: command_worker_id,
                    }
                    | NativeScriptCommand::ServiceWorkerClientsClaim {
                        worker_id: command_worker_id,
                    } => service_worker && *command_worker_id == worker_id,
                    NativeScriptCommand::ServiceWorkerOpenWindow {
                        worker_id: command_worker_id,
                        ..
                    } => service_worker && *command_worker_id == worker_id,
                    _ => false,
                };
                if !valid {
                    return Err(NativeEngineError::invalid(
                        "native Worker host command",
                        "worker command has an invalid owner or unsupported operation",
                    ));
                }
                worker_commands.push(command);
            }
            let json = ctx
                .json_stringify(value)
                .map_err(|error| NativeEngineError::Worker {
                    operation: "serialize native Worker result".into(),
                    reason: format!(
                        "native Worker result could not be serialized: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            let mut value = match json {
                Some(json) => {
                    let json = json.to_string().map_err(|_| NativeEngineError::Worker {
                        operation: "serialize native Worker result".into(),
                        reason: "native Worker result was not valid UTF-8".into(),
                    })?;
                    if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                        return Err(NativeEngineError::limit(
                            "native Worker result",
                            MAX_NATIVE_SCRIPT_RESULT_BYTES,
                            json.len(),
                        ));
                    }
                    serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                        operation: "decode native Worker result".into(),
                        reason: "native Worker result was not valid JSON".into(),
                    })?
                }
                None => serde_json::Value::Null,
            };
            if await_promise
                && let Some(object) = value.as_object_mut()
                && object.len() == 1
                && let Some(enveloped) = object.remove("value")
            {
                value = enveloped;
            }
            Ok(NativeScriptEvaluation {
                value,
                commands: worker_commands,
                top_level_await_pending,
                worker_script_error,
            })
        });
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    pub(crate) fn evaluate_service_worker_source(
        &self,
        worker_id: u32,
        worker_url: &str,
        module_name: Option<&str>,
        source: &str,
        import_script_counts: &BTreeMap<String, usize>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.set_dynamic_module_import_mode(NativeModuleDynamicImportMode::Reject)?;
        let source = rewrite_dynamic_imports_as_rejected(source)?;
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            import_script_counts,
            module_name.is_some(),
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name,
            },
            &source,
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
        )
    }

    pub(crate) fn evaluate_service_worker_fetch(
        &self,
        worker_id: u32,
        worker_url: &str,
        payload: &serde_json::Value,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: true,
            },
            Some(NativeWorkerDispatch::ServiceWorkerFetch { payload }),
        )
    }

    pub(crate) fn has_service_worker_fetch_listener(&self) -> Result<bool, NativeEngineError> {
        self.context.with(|ctx| {
            let has_listener: Function = ctx
                .globals()
                .get("__glassHasServiceWorkerFetchListener")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "inspect native ServiceWorker fetch listeners".into(),
                    reason: format!(
                        "ServiceWorker listener query is unavailable: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            has_listener
                .call(())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "inspect native ServiceWorker fetch listeners".into(),
                    reason: format!(
                        "ServiceWorker listener query failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })
        })
    }

    pub(crate) fn has_host_event_listeners(
        &self,
        event_types: &[String],
    ) -> Result<Option<bool>, NativeEngineError> {
        let payload =
            serde_json::to_string(event_types).map_err(|_| NativeEngineError::Worker {
                operation: "inspect native host event listeners".into(),
                reason: "event type list could not be encoded".into(),
            })?;
        self.context.with(|ctx| {
            let Ok(has_listeners) = ctx
                .globals()
                .get::<_, Function>("__glassHasHostEventListeners")
            else {
                return Ok(None);
            };
            has_listeners
                .call((payload,))
                .map(Some)
                .map_err(|error| NativeEngineError::Worker {
                    operation: "inspect native host event listeners".into(),
                    reason: format!(
                        "host event listener query failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })
        })
    }

    pub(crate) fn resolve_service_worker_navigation_preload(
        &self,
        worker_id: u32,
        worker_url: &str,
        request_id: u32,
        payload: &serde_json::Value,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker navigation preload request id",
                "must be positive",
            ));
        }
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::ServiceWorkerNavigationPreload {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn evaluate_service_worker_lifecycle(
        &self,
        worker_id: u32,
        worker_url: &str,
        event_type: &str,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: true,
            },
            Some(NativeWorkerDispatch::ServiceWorkerLifecycle { event_type }),
        )
    }

    pub(crate) fn evaluate_service_worker_timers(
        &self,
        worker_id: u32,
        worker_url: &str,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "globalThis.__glassRunWorkerTimers(performance.now());",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
        )
    }

    pub(crate) fn dispatch_service_worker_message(
        &self,
        event: NativeServiceWorkerMessageEvent<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativeServiceWorkerMessageEvent {
            worker_id,
            worker_url,
            data,
            transfer_ports,
            source,
            source_origin,
            is_module,
        } = event;
        validate_message_port_transfers(transfer_ports)?;
        let payload = serde_json::json!({
            "data": data,
            "transfer_ports": transfer_ports,
            "source": source,
            "source_origin": source_origin,
        });
        validate_native_message_payload(&payload, "native service worker message")?;
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: true,
            },
            Some(NativeWorkerDispatch::ServiceWorkerMessage { payload: &payload }),
        )
    }

    pub(crate) fn dispatch_service_worker_message_port(
        &self,
        worker_id: u32,
        worker_url: &str,
        bridge_key: &str,
        data: &serde_json::Value,
        transfer_ports: &[NativeMessagePortTransfer],
        object_urls: &[NativeObjectUrlTransfer],
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        validate_url_text("native service worker MessagePort bridge key", bridge_key)?;
        if bridge_key.is_empty() {
            return Err(NativeEngineError::invalid(
                "native service worker MessagePort bridge key",
                "must not be empty",
            ));
        }
        if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                bridge_key.len(),
            ));
        }
        validate_message_port_transfers(transfer_ports)?;
        validate_native_object_url_transfers(object_urls)?;
        let payload = serde_json::json!({
            "data": data,
            "transfer_ports": transfer_ports,
            "object_urls": object_urls,
        });
        validate_native_message_payload(&payload, "native service worker MessagePort event")?;
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            false,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::MessagePort {
                bridge_key,
                data,
                transfer_ports,
                object_urls,
            }),
        )
    }

    pub(crate) fn dispatch_service_worker_message_port_close(
        &self,
        worker_id: u32,
        worker_url: &str,
        bridge_key: &str,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        validate_url_text("native service worker MessagePort bridge key", bridge_key)?;
        if bridge_key.is_empty() {
            return Err(NativeEngineError::invalid(
                "native service worker MessagePort bridge key",
                "must not be empty",
            ));
        }
        if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                bridge_key.len(),
            ));
        }
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::MessagePortClose { bridge_key }),
        )
    }

    pub(crate) fn resolve_service_worker_fetch(
        &self,
        worker_id: u32,
        worker_url: &str,
        request_id: u32,
        payload: &serde_json::Value,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker fetch request id",
                "must be positive",
            ));
        }
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::Fetch {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn evaluate_service_worker_fetch_upload_event(
        &self,
        worker_id: u32,
        worker_url: &str,
        stream_id: u32,
        event: &serde_json::Value,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if stream_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker fetch upload stream id",
                "must be positive",
            ));
        }
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::FetchUpload {
                stream_id,
                payload: event,
            }),
        )
    }

    pub(crate) fn resolve_service_worker_cache(
        &self,
        worker_id: u32,
        worker_url: &str,
        request_id: u32,
        payload: &serde_json::Value,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker cache request id",
                "must be positive",
            ));
        }
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::ServiceWorkerCache {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn resolve_service_worker_open_window(
        &self,
        worker_id: u32,
        worker_url: &str,
        request_id: u32,
        payload: &serde_json::Value,
        is_module: bool,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker openWindow request id",
                "must be positive",
            ));
        }
        let bootstrap = service_worker_bootstrap(
            worker_id,
            worker_url,
            self.now_ms(),
            &BTreeMap::new(),
            is_module,
            self.worker_global_referrer_policy,
        )?;
        self.evaluate_worker_source_with_bootstrap_and_event(
            NativeWorkerSourceIdentity {
                worker_id,
                worker_url,
                module_name: None,
            },
            "undefined;",
            bootstrap,
            NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
            Some(NativeWorkerDispatch::ServiceWorkerOpenWindow {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn resolve_fetch(
        &self,
        request_id: u32,
        payload: &serde_json::Value,
        context: NativePageEventContext<'_>,
        page_events: &NativePageEventBatch,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native fetch response request id",
                "must be positive",
            ));
        }
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document: context.document,
                document_url: context.document_url,
                origin: context.origin,
                viewport: context.viewport,
            },
            page_events,
            Some(NativePageDispatch::Fetch {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn resolve_font_face_install(
        &self,
        request_id: u32,
        payload: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native FontFace install request id",
                "must be positive",
            ));
        }
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document,
                document_url,
                origin,
                viewport,
            },
            &NativePageEventBatch::default(),
            Some(NativePageDispatch::FontFaceInstall {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn resolve_service_worker_registration(
        &self,
        request_id: u32,
        payload: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker request id",
                "must be positive",
            ));
        }
        self.evaluate_with_page_events_and_dispatch(
            "undefined;",
            NativePageEvaluationContext {
                document,
                document_url,
                origin,
                viewport,
            },
            &NativePageEventBatch::default(),
            Some(NativePageDispatch::ServiceWorkerRegistration {
                request_id,
                payload,
            }),
        )
    }

    pub(crate) fn take_top_level_await_result(
        &self,
    ) -> Result<Option<serde_json::Value>, NativeEngineError> {
        self.context.with(|ctx| {
            let json: String = ctx
                .eval("JSON.stringify(globalThis.__glassTopLevelAwaitState || { state: 'none' })")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "read JavaScript evaluation promise".into(),
                    reason: format!(
                        "JavaScript evaluation state could not be read: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "script result",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            let state: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                    operation: "decode JavaScript evaluation promise".into(),
                    reason: "JavaScript evaluation state was not valid JSON".into(),
                })?;
            match state.get("state").and_then(serde_json::Value::as_str) {
                Some("none") | Some("pending") => Ok(None),
                Some("fulfilled") => {
                    ctx.eval::<(), _>("globalThis.__glassTopLevelAwaitState = null;")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "clear JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation state could not be cleared: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    let mut value = state
                        .get("value")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    if let Some(object) = value.as_object_mut()
                        && object.len() == 1
                        && let Some(enveloped) = object.remove("value")
                    {
                        value = enveloped;
                    }
                    Ok(Some(value))
                }
                Some("rejected") => {
                    let reason = state
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown JavaScript evaluation error")
                        .to_owned();
                    ctx.eval::<(), _>("globalThis.__glassTopLevelAwaitState = null;")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "clear JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation state could not be cleared: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    Err(NativeEngineError::Worker {
                        operation: "resolve JavaScript evaluation promise".into(),
                        reason,
                    })
                }
                _ => Err(NativeEngineError::Worker {
                    operation: "decode JavaScript evaluation promise".into(),
                    reason: "JavaScript evaluation state had an unknown status".into(),
                }),
            }
        })
    }

    pub(crate) fn evaluate_module(
        &self,
        name: &str,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if name.is_empty() {
            return Err(NativeEngineError::invalid(
                "module name",
                "must not be empty",
            ));
        }
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "module source",
                "must not be empty",
            ));
        }
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "module source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        self.set_dynamic_module_imports_enabled(true)?;
        let module_base_url = self
            .module_base_urls
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "read module script base URL".into(),
                reason: "native module base URL map is unavailable".into(),
            })?
            .get(name)
            .cloned()
            .unwrap_or_else(|| document_url.to_owned());
        self.register_dynamic_import_referrer(&module_base_url)?;
        let module_referrer =
            serde_json::to_string(&module_base_url).map_err(|_| NativeEngineError::Worker {
                operation: "prepare dynamic module referrer".into(),
                reason: "module base URL could not be encoded".into(),
            })?;
        let referrer_policy = self
            .module_referrer_policies
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "read module referrer policy".into(),
                reason: "native module referrer policy map is unavailable".into(),
            })?
            .get(name)
            .copied();
        let referrer_policy = native_module_referrer_policy_expression(referrer_policy);
        let source = rewrite_runtime_dynamic_module_imports_with_policy(
            source,
            &module_referrer,
            &referrer_policy,
        )?;
        let source = rewrite_static_json_module_specifiers(&source)?;
        let storage_events = self.take_storage_events();
        let proxy_updates = self.take_window_proxy_updates();
        let window_name = self.window_name();
        let opener_window_name = self.opener_window_name();
        let frame_id = self.frame_id();
        let scroll_offset = self.scroll_offset();
        let nested_scroll_offsets = self.nested_scroll_offsets();
        let history_state = self.history_state();
        let history_length = self.history_length();
        let document_creation_url = self.document_creation_url_for(document, document_url)?;
        let bootstrap = document_bootstrap(
            document,
            document_url,
            &document_creation_url,
            &frame_id,
            &window_name,
            self.opener_context_id(),
            opener_window_name,
            &self.opener_url,
            origin,
            viewport,
            scroll_offset,
            &nested_scroll_offsets,
            &history_state,
            history_length,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
            &self.indexed_db_state(),
            &storage_events,
            &self.cookie_state(),
            &self.frame_script_bindings(),
            self.frame_script_context().as_ref(),
            &self.service_worker_registrations(),
            self.timer_pump_enabled(),
            true,
            &self.environment(),
        )?;
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let result = self.context.with(|ctx| {
            install_native_crypto_sources(ctx.clone())?;
            install_native_sync_xhr_source(
                ctx.clone(),
                Arc::clone(&self.sync_xhr_loader),
                Arc::clone(&self.sync_xhr_loader_used),
                Arc::clone(&self.cookie),
                Arc::clone(&self.cookie_updates),
                Arc::clone(&self.parent_brokered_cookie_updates),
            )?;
            install_native_url_source(ctx.clone())?;
            if let Ok(mut current) = self.deadline.lock() {
                *current = Some(Instant::now() + NATIVE_HOST_VIEW_TIMEOUT);
            }
            ctx.globals()
                .set("__glassHostViewJson", bootstrap.host_state_json.as_str())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "install JavaScript host snapshot".into(),
                    reason: format!(
                        "native JavaScript host snapshot could not be installed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            ctx.eval::<(), _>(bootstrap.source.as_str())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "install JavaScript host view".into(),
                    reason: format!(
                        "native JavaScript host view could not be installed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            if let Ok(mut current) = self.deadline.lock() {
                *current = Some(Instant::now() + NATIVE_SCRIPT_TIMEOUT);
            }
            dispatch_window_proxy_updates(&ctx, &proxy_updates)?;
            Module::evaluate(ctx.clone(), name, source)
                .and_then(|promise| promise.finish::<()>())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "evaluate JavaScript module".into(),
                    reason: format!(
                        "JavaScript module evaluation failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueResizeObserverChanges === 'function') globalThis.__glassQueueResizeObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native resize observers".into(),
                reason: "native resize observers could not be scheduled".into(),
            })?;
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueIntersectionObserverChanges === 'function') globalThis.__glassQueueIntersectionObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native intersection observers".into(),
                reason: "native intersection observers could not be scheduled".into(),
            })?;
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            for (event_type, cancelable, reasons) in [
                (
                    "unhandledrejection",
                    true,
                    self.take_unhandled_promise_rejections(),
                ),
                (
                    "rejectionhandled",
                    false,
                    self.take_handled_promise_rejections(),
                ),
            ] {
                dispatch_promise_rejections(&ctx, event_type, &reasons, cancelable)?;
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut document_commands = Vec::with_capacity(commands.len());
            for command in commands {
                if self
                    .apply_storage_command(&command, document_url, origin)?
                    .is_some()
                {
                    continue;
                }
                if self.apply_cookie_command(&command)? {
                    continue;
                }
                if self.apply_dialog_command(&command)? {
                    continue;
                }
                if self.apply_popup_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_window_name_command(&command)? {
                    continue;
                }
                if self.apply_window_close_command(&command)? {
                    continue;
                }
                if self.apply_window_navigation_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_frame_script_command(&command)? {
                    continue;
                }
                if self.apply_post_message_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_message_port_command(&ctx, &command)? {
                    continue;
                }
                if self.apply_service_worker_command(&command)? {
                    continue;
                }
                if self.apply_worker_command(&ctx, &command)? {
                    continue;
                }
                document_commands.push(command);
            }
            let indexed_db_state = read_indexed_db_state(ctx.clone())?;
            self.set_indexed_db_state(indexed_db_state);
            Ok(NativeScriptEvaluation {
                value: serde_json::Value::Null,
                commands: document_commands,
                top_level_await_pending: false,
                worker_script_error: None,
            })
        });
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    #[allow(dead_code)]
    pub(crate) fn has_pending_jobs(&self) -> bool {
        self.runtime.is_job_pending()
    }
}
