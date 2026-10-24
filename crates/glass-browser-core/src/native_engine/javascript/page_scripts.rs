use super::*;

pub(crate) struct NativeLocalModuleGraphRequest<'a> {
    pub(crate) loader: &'a NativeResourceLoader,
    pub(crate) document_url: &'a str,
    pub(crate) root_name: String,
    pub(crate) root_base_url: String,
    pub(crate) root_source: String,
    pub(crate) timing: NativePageScriptTiming,
    pub(crate) node_index: Option<u32>,
    pub(crate) import_map: &'a mut NativeModuleImportMap,
    pub(crate) existing_module_names: &'a BTreeSet<String>,
    pub(crate) existing_module_bytes: usize,
}

pub(crate) struct NativeDynamicPageScriptRequest<'a> {
    pub(crate) document: &'a mut NativeDocument,
    pub(crate) runtime: &'a NativeJavaScriptRuntime,
    pub(crate) sources: Vec<NativePageScript>,
    pub(crate) document_url: &'a str,
    pub(crate) document_origin: &'a NativeOrigin,
    pub(crate) viewport: Viewport,
    pub(crate) resource_events: &'a [(u32, NativeEventKind)],
    pub(crate) csp_violations: &'a [NativeCspViolation],
}

pub(crate) struct NativeInlinePageScriptRequest<'a> {
    pub(crate) document: &'a mut NativeDocument,
    pub(crate) runtime: &'a mut Option<NativeJavaScriptRuntime>,
    pub(crate) context_id: &'a str,
    pub(crate) document_url: &'a str,
    pub(crate) document_origin: &'a NativeOrigin,
    pub(crate) viewport: Viewport,
    pub(crate) storage_state: &'a NativeWebStorageState,
    pub(crate) indexed_db_state: &'a NativeIndexedDbState,
    pub(crate) cookie: &'a str,
    pub(crate) loader: &'a mut NativeResourceLoader,
}

/// Execute the bounded inline scripts discovered in one parsed document.
///
/// The caller owns the realm so local documents and the child content process
/// can both retain globals and listeners after the document commit. Script
/// navigation is returned as a typed handoff for the current navigation owner;
/// it is never executed recursively inside the JavaScript evaluator.
pub(crate) fn execute_inline_scripts(
    request: NativeInlinePageScriptRequest<'_>,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let NativeInlinePageScriptRequest {
        document,
        runtime,
        context_id,
        document_url,
        document_origin,
        viewport,
        storage_state,
        indexed_db_state,
        cookie,
        loader,
    } = request;
    if runtime.is_none() {
        *runtime = Some(NativeJavaScriptRuntime::new_with_context_id(context_id)?);
    }
    let current_runtime = runtime.as_ref().expect("inline page runtime initialized");
    current_runtime.set_inline_script_policy(loader.inline_script_policy(document_url)?);
    current_runtime.mark_processed_inline_csp_meta_nodes(
        document.processed_content_security_policy_meta_nodes(),
    );
    let mut resource_events = Vec::new();
    let mut module_dependency_sources = Vec::new();
    let mut import_map = runtime
        .as_ref()
        .map(NativeJavaScriptRuntime::module_import_map)
        .transpose()?
        .unwrap_or_default();
    let mut sources = vec![(
        NativePageScriptTiming::ParserBlocking,
        NativePageScript::ImportMap(import_map.clone()),
    )];
    let mut page_import_maps = document
        .page_import_map_sources(MAX_NATIVE_INLINE_SCRIPTS, MAX_NATIVE_SCRIPT_BYTES)
        .into_iter()
        .peekable();
    for (index, source) in document
        .page_script_sources(MAX_NATIVE_INLINE_SCRIPTS, MAX_NATIVE_SCRIPT_BYTES)
        .into_iter()
        .enumerate()
    {
        let node_index = source.node_index();
        while page_import_maps
            .peek()
            .is_some_and(|map_source| map_source.node_index < node_index)
        {
            let map_source = page_import_maps
                .next()
                .expect("peeked import-map source remains available");
            register_local_page_import_map_source(
                map_source,
                &mut import_map,
                loader,
                document_url,
                &mut sources,
                &mut resource_events,
            )?;
        }
        match source {
            NativePageScriptSource::Inline {
                source,
                timing,
                node_index,
                nonce,
                parser_inserted: _,
            } => {
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                sources.push((
                    timing,
                    NativePageScript::Classic {
                        source,
                        base_url: document_url.to_owned(),
                        node_index: Some(node_index),
                    },
                ));
            }
            NativePageScriptSource::ModuleInline {
                source,
                timing,
                node_index,
                nonce,
                referrer_policy: _,
                parser_inserted: _,
            } => {
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                let name = format!("{document_url}#glass-inline-module-{index}");
                match load_local_file_module_graph_with_import_map(NativeLocalModuleGraphRequest {
                    loader,
                    document_url,
                    root_name: name,
                    root_base_url: document_url.to_owned(),
                    root_source: source,
                    timing,
                    node_index: Some(node_index),
                    import_map: &mut import_map,
                    existing_module_names: &BTreeSet::new(),
                    existing_module_bytes: 0,
                }) {
                    Ok(mut graph) => {
                        sources.push(graph.remove(0));
                        module_dependency_sources.extend(graph);
                        resource_events.push((node_index, NativeEventKind::Load));
                    }
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
                sources.push((
                    NativePageScriptTiming::ParserBlocking,
                    NativePageScript::ImportMap(import_map.clone()),
                ));
            }
            NativePageScriptSource::External {
                href,
                timing,
                node_index,
                nonce,
                integrity,
                parser_inserted,
                ..
            } if is_file_url(document_url) => {
                match loader.load_local_file_script_with_policy(
                    document_url,
                    &href,
                    MAX_NATIVE_SCRIPT_BYTES,
                    parser_inserted,
                    nonce.as_deref(),
                    integrity.as_deref(),
                ) {
                    Ok(Some(resource)) => {
                        resource_events.push((node_index, NativeEventKind::Load));
                        sources.push((
                            timing,
                            NativePageScript::Classic {
                                source: resource.body,
                                base_url: resource.url,
                                node_index: Some(node_index),
                            },
                        ));
                    }
                    Ok(None) | Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
            NativePageScriptSource::ModuleExternal {
                href,
                timing,
                node_index,
                nonce,
                integrity,
                parser_inserted,
                ..
            } if is_file_url(document_url) => {
                let request_url = match resolve_local_file_module_script_url(document_url, &href) {
                    Ok(request_url) => request_url,
                    Err(_) => {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    }
                };
                match loader.load_local_file_script_with_policy(
                    document_url,
                    &request_url,
                    MAX_NATIVE_SCRIPT_BYTES,
                    parser_inserted,
                    nonce.as_deref(),
                    integrity.as_deref(),
                ) {
                    Ok(Some(resource)) => {
                        match load_local_file_module_graph_with_import_map(
                            NativeLocalModuleGraphRequest {
                                loader,
                                document_url,
                                root_name: request_url,
                                root_base_url: resource.url,
                                root_source: resource.body,
                                timing,
                                node_index: Some(node_index),
                                import_map: &mut import_map,
                                existing_module_names: &BTreeSet::new(),
                                existing_module_bytes: 0,
                            },
                        ) {
                            Ok(mut graph) => {
                                sources.push(graph.remove(0));
                                module_dependency_sources.extend(graph);
                                resource_events.push((node_index, NativeEventKind::Load));
                            }
                            Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                        }
                        sources.push((
                            NativePageScriptTiming::ParserBlocking,
                            NativePageScript::ImportMap(import_map.clone()),
                        ));
                    }
                    Ok(None) | Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
            NativePageScriptSource::External { .. }
            | NativePageScriptSource::ModuleExternal { .. }
            | NativePageScriptSource::ImportMap { .. } => {}
        }
    }
    for map_source in page_import_maps {
        register_local_page_import_map_source(
            map_source,
            &mut import_map,
            loader,
            document_url,
            &mut sources,
            &mut resource_events,
        )?;
    }
    let mut sources = order_page_scripts(sources);
    sources.extend(
        module_dependency_sources
            .into_iter()
            .map(|(_, source)| source),
    );
    execute_page_scripts(
        document,
        runtime,
        context_id,
        None,
        &sources,
        document_url,
        document_origin,
        viewport,
        storage_state,
        indexed_db_state,
        cookie,
        &resource_events,
        &loader.take_csp_violations(),
    )
}

pub(crate) fn load_local_file_module_graph_with_import_map(
    request: NativeLocalModuleGraphRequest<'_>,
) -> Result<Vec<(NativePageScriptTiming, NativePageScript)>, NativeEngineError> {
    load_local_file_module_graph_with_import_map_and_existing(
        request.loader,
        request.document_url,
        request.root_name,
        request.root_base_url,
        request.root_source,
        request.timing,
        request.node_index,
        request.existing_module_names,
        request.existing_module_bytes,
        request.import_map,
    )
}

pub(crate) fn load_local_file_dynamic_module_graph_with_import_map(
    mut request: NativeLocalModuleGraphRequest<'_>,
) -> Result<Vec<(NativePageScriptTiming, NativePageScript)>, NativeEngineError> {
    request.timing = NativePageScriptTiming::ParserBlocking;
    request.node_index = None;
    load_local_file_module_graph_with_import_map_and_existing(
        request.loader,
        request.document_url,
        request.root_name,
        request.root_base_url,
        request.root_source,
        request.timing,
        request.node_index,
        request.existing_module_names,
        request.existing_module_bytes,
        request.import_map,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load_local_file_module_graph_with_import_map_and_existing(
    loader: &NativeResourceLoader,
    document_url: &str,
    root_name: String,
    root_base_url: String,
    root_source: String,
    timing: NativePageScriptTiming,
    node_index: Option<u32>,
    existing_module_names: &BTreeSet<String>,
    existing_module_bytes: usize,
    import_map: &mut NativeModuleImportMap,
) -> Result<Vec<(NativePageScriptTiming, NativePageScript)>, NativeEngineError> {
    let mut sources = vec![(
        timing,
        NativePageScript::Module {
            name: root_name.clone(),
            source: root_source.clone(),
            base_url: root_base_url.clone(),
            referrer_policy: None,
            node_index,
        },
    )];
    if !is_file_url(document_url) {
        return Ok(sources);
    }

    let mut seen = existing_module_names.clone();
    seen.insert(root_name.clone());
    if seen.len() > MAX_NATIVE_MODULE_IMPORTS {
        return Err(NativeEngineError::limit(
            "native file module graph entries",
            MAX_NATIVE_MODULE_IMPORTS,
            seen.len(),
        ));
    }
    let mut pending = vec![(root_name, root_base_url, root_source.clone())];
    let mut import_edges = 0usize;
    let mut total_bytes = existing_module_bytes.saturating_add(root_source.len());
    let maximum_module_bytes = MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS);
    if total_bytes > maximum_module_bytes {
        return Err(NativeEngineError::limit(
            "native file module graph bytes",
            maximum_module_bytes,
            total_bytes,
        ));
    }
    while let Some((module_name, module_base_url, module_source)) = pending.pop() {
        if module_name.starts_with(NATIVE_JSON_MODULE_NAME_PREFIX) {
            continue;
        }
        let requests = static_module_requests(&module_source)?;
        for request in requests {
            if request.module_type == NativeModuleResourceType::Unsupported {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "file module import attributes request an unsupported module type"
                        .into(),
                });
            }
            import_edges = import_edges.saturating_add(1);
            if import_edges > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "native file module imports",
                    MAX_NATIVE_MODULE_IMPORTS,
                    import_edges,
                ));
            }
            let target = import_map
                .resolve_and_record(&module_base_url, &request.specifier)
                .map_err(|reason| NativeEngineError::Network {
                    operation: "file module dependency".into(),
                    reason: reason.into(),
                })?;
            let name = native_module_loader_name(&target, request.module_type);
            if !seen.insert(name.clone()) {
                continue;
            }
            if seen.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "native file module graph entries",
                    MAX_NATIVE_MODULE_IMPORTS,
                    seen.len(),
                ));
            }
            let resource = loader
                .load_local_file_script_with_policy(
                    document_url,
                    &target,
                    MAX_NATIVE_SCRIPT_BYTES,
                    true,
                    None,
                    import_map.integrity_for_url(&target),
                )?
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "file module dependency".into(),
                    reason: format!(
                        "file module dependency {:?} was blocked or unavailable",
                        request.specifier
                    ),
                })?;
            total_bytes = total_bytes.saturating_add(resource.body.len());
            if total_bytes > maximum_module_bytes {
                return Err(NativeEngineError::limit(
                    "native file module graph bytes",
                    maximum_module_bytes,
                    total_bytes,
                ));
            }
            let base_url = resource.url;
            let source = resource.body;
            sources.push((
                timing,
                NativePageScript::ModuleDependency {
                    name: name.clone(),
                    source: source.clone(),
                    base_url: base_url.clone(),
                    referrer_policy: None,
                },
            ));
            pending.push((name, base_url, source));
        }
    }
    Ok(sources)
}

pub(crate) fn resolve_module_request_url(
    base_url: &str,
    specifier: &str,
) -> Result<String, NativeEngineError> {
    let base = Url::parse(base_url).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "module base URL is not valid URL syntax".into(),
    })?;
    let target = base
        .join(specifier)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "module request URL could not be resolved against its base".into(),
        })?;
    if !target.username().is_empty() || target.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "module request URL must not contain credentials".into(),
        });
    }
    Ok(target.to_string())
}

pub(super) fn resolve_local_file_module_script_url(
    document_url: &str,
    src: &str,
) -> Result<String, NativeEngineError> {
    let base = Url::parse(without_fragment(document_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "file module document URL is not valid URL syntax".into(),
        }
    })?;
    if !is_file_url(base.as_str()) {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "rooted file module document URL must use the file scheme".into(),
        });
    }
    let target = base
        .join(src)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "file module script URL could not be resolved against its document".into(),
        })?;
    if !target.username().is_empty() || target.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file module script URL must not contain credentials".into(),
        });
    }
    if !is_file_url(target.as_str()) {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "rooted file module script URL must use the file scheme".into(),
        });
    }
    Ok(target.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_page_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    context_id: &str,
    trace_request_id: Option<u64>,
    sources: &[NativePageScript],
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    cookie: &str,
    resource_events: &[(u32, NativeEventKind)],
    csp_violations: &[NativeCspViolation],
) -> Result<NativePageScriptResult, NativeEngineError> {
    document.mark_attached_scripts_started();
    let runtime_initialization_stage = trace_request_id.map(|request_id| {
        crate::native_engine::content_process::ContentLoadStage::begin(
            request_id,
            "page_script_runtime_initialization",
        )
    });
    if runtime.is_none() {
        *runtime = Some(NativeJavaScriptRuntime::new_with_context_id(context_id)?);
    }
    if let Some(stage) = runtime_initialization_stage {
        stage.finish(None);
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_storage_state(storage_state.clone());
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_indexed_db_state(indexed_db_state.origin(&storage_key(document_url, document_origin)));
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_cookie_state(cookie.to_owned());
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_timer_pump_enabled(false);
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("loading");
    let module_setup_stage = trace_request_id.map(|request_id| {
        crate::native_engine::content_process::ContentLoadStage::begin(
            request_id,
            "page_script_module_setup",
        )
    });
    let (module_sources, module_base_urls, module_referrer_policies) = module_source_maps(sources);
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_module_sources(module_sources);
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_module_base_urls(module_base_urls);
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_module_referrer_policies(module_referrer_policies);
    if let Some(stage) = module_setup_stage {
        stage.finish(Some(sources.len()));
    }
    let mut pending_fetches = Vec::new();
    let mut websocket_commands = Vec::new();
    let mut event_source_commands = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut navigation = None;
    let mut events = Vec::new();
    let mut failed_script_nodes = BTreeSet::new();
    let mut pending_dynamic_scripts = VecDeque::new();
    let mut pending_script_sources = Vec::new();
    let mut service_worker_commands = Vec::new();
    let mut dialogs = Vec::new();
    for (script_ordinal, source) in sources.iter().enumerate() {
        if let NativePageScript::ImportMap(import_map) = source {
            runtime
                .as_ref()
                .expect("page script runtime initialized")
                .set_module_import_map(import_map.clone());
            continue;
        }
        let script_node_index = match source {
            NativePageScript::Classic { node_index, .. }
            | NativePageScript::Module { node_index, .. } => *node_index,
            NativePageScript::ModuleDependency { .. } | NativePageScript::ImportMap(_) => None,
        };
        let script_stage = trace_request_id.map(|request_id| {
            crate::native_engine::content_process::ContentLoadStage::begin_with_resource(
                request_id,
                "page_script_evaluation",
                Some(script_ordinal),
                Some(match source {
                    NativePageScript::Classic { .. } => "classic",
                    NativePageScript::Module { .. } => "module",
                    NativePageScript::ModuleDependency { .. } => "module_dependency",
                    NativePageScript::ImportMap(_) => "import_map",
                }),
            )
        });
        let evaluation = {
            let script_runtime = runtime.as_ref().expect("page script runtime initialized");
            match source {
                NativePageScript::Classic {
                    source, base_url, ..
                } => script_runtime.evaluate_classic_script(
                    source,
                    base_url,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                ),
                NativePageScript::Module { name, source, .. } => script_runtime.evaluate_module(
                    name,
                    source,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                ),
                NativePageScript::ModuleDependency { .. } | NativePageScript::ImportMap(_) => {
                    continue;
                }
            }
        };
        if let Some(stage) = script_stage {
            stage.finish(None);
        }
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) if is_ignorable_page_script_error(&error) => {
                if let Some(node_index) = script_node_index {
                    failed_script_nodes.insert(node_index);
                    let message = page_script_error_message(&error);
                    let error_evaluation = runtime
                        .as_ref()
                        .expect("page script runtime initialized")
                        .dispatch_script_error_event(
                            Some(node_index),
                            &message,
                            document_url,
                            NativePageEventContext {
                                document,
                                document_url,
                                origin: document_origin,
                                viewport,
                                csp_violations: &[],
                            },
                        )?;
                    let commands = error_evaluation.commands.clone();
                    apply_page_script_evaluation(
                        document,
                        runtime.as_ref().expect("page script runtime initialized"),
                        NativePageScriptContext {
                            document_url,
                            origin: document_origin,
                            viewport,
                        },
                        error_evaluation,
                        NativePageScriptCommandQueues {
                            pending_fetches: &mut pending_fetches,
                            websocket_commands: &mut websocket_commands,
                            event_source_commands: &mut event_source_commands,
                        },
                        &mut scroll_commands,
                        &mut navigation,
                    )?;
                    enqueue_dynamic_page_scripts(
                        document,
                        &commands,
                        document_url,
                        &mut pending_dynamic_scripts,
                        &mut pending_script_sources,
                    )?;
                    events.push((node_index, NativeEventKind::Error));
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let commands = evaluation.commands.clone();
        let apply_stage = trace_request_id.map(|request_id| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                request_id,
                "page_script_command_apply",
            )
        });
        apply_page_script_evaluation(
            document,
            runtime.as_ref().expect("page script runtime initialized"),
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending_dynamic_scripts,
            &mut pending_script_sources,
        )?;
        if let Some(stage) = apply_stage {
            stage.finish(Some(commands.len()));
        }
    }
    if !csp_violations.is_empty() {
        let page_events = NativePageEventBatch {
            csp_violations: csp_violations.to_vec(),
            ..NativePageEventBatch::default()
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate_with_page_events(
                "undefined;",
                document,
                document_url,
                document_origin,
                viewport,
                &page_events,
            )?;
        let commands = evaluation.commands.clone();
        let apply_stage = trace_request_id.map(|request_id| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                request_id,
                "page_csp_event_apply",
            )
        });
        apply_page_script_evaluation(
            document,
            runtime.as_ref().expect("page script runtime initialized"),
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending_dynamic_scripts,
            &mut pending_script_sources,
        )?;
        if let Some(stage) = apply_stage {
            stage.finish(Some(commands.len()));
        }
    }
    let resource_event_stage = trace_request_id.map(|request_id| {
        crate::native_engine::content_process::ContentLoadStage::begin(
            request_id,
            "page_resource_event_dispatch",
        )
    });
    for (node_index, event_kind) in resource_events {
        if *event_kind == NativeEventKind::Load && failed_script_nodes.contains(node_index) {
            continue;
        }
        let Some(event_batch) = host_event_batch(&[(*node_index, *event_kind)])? else {
            continue;
        };
        let event_types = event_batch
            .iter()
            .map(|event| event.event_type.clone())
            .collect::<Vec<_>>();
        let has_listeners = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .has_host_event_listeners(&event_types)?;
        if !event_types
            .iter()
            .any(|event_type| document.has_inline_event_handler(event_type))
            && has_listeners == Some(false)
        {
            continue;
        }
        let event_stage = trace_request_id.map(|request_id| {
            let stage = match event_kind {
                NativeEventKind::Load => "page_resource_load_handler",
                NativeEventKind::Error => "page_resource_error_handler",
                _ => "page_resource_other_handler",
            };
            crate::native_engine::content_process::ContentLoadStage::begin(request_id, stage)
        });
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate_with_host_events(
                &event_batch,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        if let Some(stage) = event_stage {
            stage.finish(Some(event_types.len()));
        }
        let commands = evaluation.commands.clone();
        let apply_stage = trace_request_id.map(|request_id| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                request_id,
                "page_resource_event_apply",
            )
        });
        apply_page_script_evaluation(
            document,
            runtime.as_ref().expect("page script runtime initialized"),
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending_dynamic_scripts,
            &mut pending_script_sources,
        )?;
        if let Some(stage) = apply_stage {
            stage.finish(Some(commands.len()));
        }
        events.push((*node_index, *event_kind));
    }
    if let Some(stage) = resource_event_stage {
        stage.finish(Some(resource_events.len()));
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("interactive");
    let dom_content_loaded_stage = trace_request_id.map(|request_id| {
        crate::native_engine::content_process::ContentLoadStage::begin(
            request_id,
            "dom_content_loaded_dispatch",
        )
    });
    for (target, kind) in [
        (0, NativeEventKind::ReadyStateChange),
        (0, NativeEventKind::DomContentLoaded),
    ] {
        let Some(event_batch) = host_event_batch(&[(target, kind)])? else {
            continue;
        };
        let event_types = event_batch
            .iter()
            .map(|event| event.event_type.clone())
            .collect::<Vec<_>>();
        let has_listeners = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .has_host_event_listeners(&event_types)?;
        if !event_types
            .iter()
            .any(|event_type| document.has_inline_event_handler(event_type))
            && has_listeners == Some(false)
        {
            continue;
        }
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate_with_host_events(
                &event_batch,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        let commands = evaluation.commands.clone();
        let apply_stage = trace_request_id.map(|request_id| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                request_id,
                "dom_content_loaded_apply",
            )
        });
        apply_page_script_evaluation(
            document,
            runtime.as_ref().expect("page script runtime initialized"),
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending_dynamic_scripts,
            &mut pending_script_sources,
        )?;
        if let Some(stage) = apply_stage {
            stage.finish(Some(commands.len()));
        }
        events.push((target, kind));
    }
    if let Some(stage) = dom_content_loaded_stage {
        stage.finish(None);
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("complete");
    let load_event_stage = trace_request_id.map(|request_id| {
        crate::native_engine::content_process::ContentLoadStage::begin(
            request_id,
            "page_load_event_dispatch",
        )
    });
    for (target, kind) in [
        (0, NativeEventKind::ReadyStateChange),
        (u32::MAX, NativeEventKind::Load),
    ] {
        let Some(event_batch) = host_event_batch(&[(target, kind)])? else {
            continue;
        };
        let event_types = event_batch
            .iter()
            .map(|event| event.event_type.clone())
            .collect::<Vec<_>>();
        let has_listeners = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .has_host_event_listeners(&event_types)?;
        if !event_types
            .iter()
            .any(|event_type| document.has_inline_event_handler(event_type))
            && has_listeners == Some(false)
        {
            continue;
        }
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate_with_host_events(
                &event_batch,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        let commands = evaluation.commands.clone();
        let apply_stage = trace_request_id.map(|request_id| {
            crate::native_engine::content_process::ContentLoadStage::begin(
                request_id,
                "page_load_event_apply",
            )
        });
        apply_page_script_evaluation(
            document,
            runtime.as_ref().expect("page script runtime initialized"),
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending_dynamic_scripts,
            &mut pending_script_sources,
        )?;
        if let Some(stage) = apply_stage {
            stage.finish(Some(commands.len()));
        }
        events.push((target, kind));
    }
    if let Some(stage) = load_event_stage {
        stage.finish(None);
    }
    if !pending_dynamic_scripts.is_empty() {
        let dynamic_result = execute_dynamic_page_scripts(NativeDynamicPageScriptRequest {
            document,
            runtime: runtime.as_ref().expect("page script runtime initialized"),
            sources: pending_dynamic_scripts.into_iter().collect(),
            document_url,
            document_origin,
            viewport,
            resource_events: &[],
            csp_violations: &[],
        })?;
        pending_fetches.extend(dynamic_result.pending_fetches);
        websocket_commands.extend(dynamic_result.websocket_commands);
        event_source_commands.extend(dynamic_result.event_source_commands);
        pending_script_sources.extend(dynamic_result.pending_script_sources);
        scroll_commands.extend(dynamic_result.scroll_commands);
        service_worker_commands.extend(dynamic_result.service_worker_commands);
        dialogs.extend(dynamic_result.dialogs);
        events.extend(dynamic_result.events);
        if let Some(dynamic_navigation) = dynamic_result.navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one page-load script batch cannot activate multiple navigations"
                        .into(),
                });
            }
            navigation = Some(dynamic_navigation);
        }
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_timer_pump_enabled(true);
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .reset_timer_clock();
    dispatch_page_scroll_events(
        document,
        runtime.as_ref().expect("page script runtime initialized"),
        NativePageScriptContext {
            document_url,
            origin: document_origin,
            viewport,
        },
        &mut scroll_commands,
        NativePageScriptCommandQueues {
            pending_fetches: &mut pending_fetches,
            websocket_commands: &mut websocket_commands,
            event_source_commands: &mut event_source_commands,
        },
        &mut navigation,
        &mut events,
    )?;
    if let Some(navigation) = navigation.as_mut() {
        navigation.object_url = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .object_url_resource(&navigation.href)?;
    }
    Ok(NativePageScriptResult {
        pending_fetches,
        service_worker_commands: service_worker_commands
            .into_iter()
            .chain(
                runtime
                    .as_ref()
                    .expect("page script runtime initialized")
                    .take_service_worker_commands(),
            )
            .collect(),
        websocket_commands,
        event_source_commands,
        pending_script_sources,
        scroll_commands,
        navigation,
        dialogs: dialogs
            .into_iter()
            .chain(
                runtime
                    .as_ref()
                    .expect("page script runtime initialized")
                    .take_dialog_events(),
            )
            .collect(),
        events,
    })
}

/// Execute scripts attached by a completed DOM mutation. Unlike the initial
/// page loader this path does not replay document lifecycle events; it only
/// runs each newly started script and the resource event associated with it.
/// The document owns the single-shot ledger, so a script created in one host
/// batch and attached in a later batch still executes exactly once.
pub(crate) fn execute_dynamic_page_scripts(
    request: NativeDynamicPageScriptRequest<'_>,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let NativeDynamicPageScriptRequest {
        document,
        runtime,
        sources,
        document_url,
        document_origin,
        viewport,
        resource_events,
        csp_violations,
    } = request;
    let (new_module_sources, new_module_base_urls, new_module_referrer_policies) =
        module_source_maps(&sources);
    let (mut module_sources, mut module_base_urls) = runtime.module_sources_snapshot()?;
    let mut module_referrer_policies = runtime.module_referrer_policies_snapshot()?;
    for (name, source) in new_module_sources {
        module_sources.entry(name).or_insert(source);
    }
    for (name, base_url) in new_module_base_urls {
        module_base_urls.entry(name).or_insert(base_url);
    }
    for (name, referrer_policy) in new_module_referrer_policies {
        module_referrer_policies
            .entry(name)
            .or_insert(referrer_policy);
    }
    if module_sources.len() > MAX_NATIVE_MODULE_IMPORTS {
        return Err(NativeEngineError::limit(
            "native page module graph entries",
            MAX_NATIVE_MODULE_IMPORTS,
            module_sources.len(),
        ));
    }
    let module_bytes = module_sources
        .values()
        .map(String::len)
        .fold(0usize, usize::saturating_add);
    let maximum_module_bytes = MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS);
    if module_bytes > maximum_module_bytes {
        return Err(NativeEngineError::limit(
            "native page module graph bytes",
            maximum_module_bytes,
            module_bytes,
        ));
    }
    runtime.set_module_sources(module_sources);
    runtime.set_module_base_urls(module_base_urls);
    runtime.set_module_referrer_policies(module_referrer_policies);

    let mut pending = VecDeque::from(sources);
    let mut pending_fetches = Vec::new();
    let mut websocket_commands = Vec::new();
    let mut event_source_commands = Vec::new();
    let mut pending_script_sources = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut navigation = None;
    let mut events = Vec::new();
    let mut failed_script_nodes = BTreeSet::new();
    let mut executed = 0usize;

    if !csp_violations.is_empty() {
        let page_events = NativePageEventBatch {
            csp_violations: csp_violations.to_vec(),
            ..NativePageEventBatch::default()
        };
        let evaluation = runtime.evaluate_with_page_events(
            "undefined;",
            document,
            document_url,
            document_origin,
            viewport,
            &page_events,
        )?;
        apply_page_script_evaluation(
            document,
            runtime,
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
    }

    while let Some(source) = pending.pop_front() {
        if let NativePageScript::ImportMap(import_map) = &source {
            runtime.set_module_import_map(import_map.clone());
            continue;
        }
        if matches!(&source, NativePageScript::ModuleDependency { .. }) {
            continue;
        }
        executed = executed.saturating_add(1);
        if executed > MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic page scripts",
                MAX_NATIVE_INLINE_SCRIPTS,
                executed,
            ));
        }
        let node_index = match &source {
            NativePageScript::Classic { node_index, .. }
            | NativePageScript::Module { node_index, .. } => *node_index,
            NativePageScript::ModuleDependency { .. } | NativePageScript::ImportMap(_) => None,
        };
        let evaluation = match &source {
            NativePageScript::Classic {
                source, base_url, ..
            } => runtime.evaluate_classic_script(
                source,
                base_url,
                document,
                document_url,
                document_origin,
                viewport,
            ),
            NativePageScript::Module { name, source, .. } => runtime.evaluate_module(
                name,
                source,
                document,
                document_url,
                document_origin,
                viewport,
            ),
            NativePageScript::ModuleDependency { .. } | NativePageScript::ImportMap(_) => continue,
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) if is_ignorable_page_script_error(&error) => {
                if let Some(node_index) = node_index {
                    failed_script_nodes.insert(node_index);
                    let message = page_script_error_message(&error);
                    let error_evaluation = runtime.dispatch_script_error_event(
                        Some(node_index),
                        &message,
                        document_url,
                        NativePageEventContext {
                            document,
                            document_url,
                            origin: document_origin,
                            viewport,
                            csp_violations: &[],
                        },
                    )?;
                    let commands = error_evaluation.commands.clone();
                    apply_page_script_evaluation(
                        document,
                        runtime,
                        NativePageScriptContext {
                            document_url,
                            origin: document_origin,
                            viewport,
                        },
                        error_evaluation,
                        NativePageScriptCommandQueues {
                            pending_fetches: &mut pending_fetches,
                            websocket_commands: &mut websocket_commands,
                            event_source_commands: &mut event_source_commands,
                        },
                        &mut scroll_commands,
                        &mut navigation,
                    )?;
                    enqueue_dynamic_page_scripts(
                        document,
                        &commands,
                        document_url,
                        &mut pending,
                        &mut pending_script_sources,
                    )?;
                    events.push((node_index, NativeEventKind::Error));
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let commands = evaluation.commands.clone();
        apply_page_script_evaluation(
            document,
            runtime,
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending,
            &mut pending_script_sources,
        )?;
    }

    for (node_index, event_kind) in resource_events {
        if *event_kind == NativeEventKind::Load && failed_script_nodes.contains(node_index) {
            continue;
        }
        let Some(event_batch) = host_event_batch(&[(*node_index, *event_kind)])? else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            document,
            document_url,
            document_origin,
            viewport,
        )?;
        let commands = evaluation.commands.clone();
        apply_page_script_evaluation(
            document,
            runtime,
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending,
            &mut pending_script_sources,
        )?;
        events.push((*node_index, *event_kind));
    }

    while let Some(source) = pending.pop_front() {
        if let NativePageScript::ImportMap(import_map) = &source {
            runtime.set_module_import_map(import_map.clone());
            continue;
        }
        if matches!(&source, NativePageScript::ModuleDependency { .. }) {
            continue;
        }
        executed = executed.saturating_add(1);
        if executed > MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic page scripts",
                MAX_NATIVE_INLINE_SCRIPTS,
                executed,
            ));
        }
        let node_index = match &source {
            NativePageScript::Classic { node_index, .. }
            | NativePageScript::Module { node_index, .. } => *node_index,
            NativePageScript::ModuleDependency { .. } | NativePageScript::ImportMap(_) => None,
        };
        let evaluation = match &source {
            NativePageScript::Classic {
                source, base_url, ..
            } => runtime.evaluate_classic_script(
                source,
                base_url,
                document,
                document_url,
                document_origin,
                viewport,
            ),
            NativePageScript::Module { name, source, .. } => runtime.evaluate_module(
                name,
                source,
                document,
                document_url,
                document_origin,
                viewport,
            ),
            NativePageScript::ModuleDependency { .. } | NativePageScript::ImportMap(_) => continue,
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) if is_ignorable_page_script_error(&error) => {
                if let Some(node_index) = node_index {
                    let message = page_script_error_message(&error);
                    let error_evaluation = runtime.dispatch_script_error_event(
                        Some(node_index),
                        &message,
                        document_url,
                        NativePageEventContext {
                            document,
                            document_url,
                            origin: document_origin,
                            viewport,
                            csp_violations: &[],
                        },
                    )?;
                    let commands = error_evaluation.commands.clone();
                    apply_page_script_evaluation(
                        document,
                        runtime,
                        NativePageScriptContext {
                            document_url,
                            origin: document_origin,
                            viewport,
                        },
                        error_evaluation,
                        NativePageScriptCommandQueues {
                            pending_fetches: &mut pending_fetches,
                            websocket_commands: &mut websocket_commands,
                            event_source_commands: &mut event_source_commands,
                        },
                        &mut scroll_commands,
                        &mut navigation,
                    )?;
                    enqueue_dynamic_page_scripts(
                        document,
                        &commands,
                        document_url,
                        &mut pending,
                        &mut pending_script_sources,
                    )?;
                    events.push((node_index, NativeEventKind::Error));
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let commands = evaluation.commands.clone();
        apply_page_script_evaluation(
            document,
            runtime,
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches: &mut pending_fetches,
                websocket_commands: &mut websocket_commands,
                event_source_commands: &mut event_source_commands,
            },
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending,
            &mut pending_script_sources,
        )?;
    }

    if let Some(navigation) = navigation.as_mut() {
        navigation.object_url = runtime.object_url_resource(&navigation.href)?;
    }

    Ok(NativePageScriptResult {
        pending_fetches,
        service_worker_commands: runtime.take_service_worker_commands(),
        websocket_commands,
        event_source_commands,
        pending_script_sources,
        scroll_commands,
        navigation,
        dialogs: runtime.take_dialog_events(),
        events,
    })
}
