use super::*;

type DynamicScriptResourceEvents = (Vec<NativePageScript>, Vec<(u32, NativeEventKind)>);
type InlineStyleSourceState = (Vec<(u32, String, Option<String>)>, Vec<(u32, String)>);
type InitialStylesheetState = (
    Vec<(u32, String, Option<String>, Option<String>)>,
    Vec<(u32, NativeEventKind)>,
);

struct NativeStylesheetImportGraph<'a> {
    entries: usize,
    bytes: usize,
    active: BTreeSet<String>,
    loaded: BTreeSet<String>,
    total_bytes: &'a mut usize,
}

pub(super) fn extract_local_history_commands(
    commands: &[NativeScriptCommand],
) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryPushState { .. }
                    | NativeScriptCommand::HistoryReplaceState { .. }
                    | NativeScriptCommand::HistoryGo { .. }
            )
        })
        .cloned()
        .collect()
}

pub(super) fn load_local_dynamic_page_script_sources(
    sources: Vec<NativePageScriptSource>,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<DynamicScriptResourceEvents, NativeEngineError> {
    let mut import_map = runtime.module_import_map()?;
    let mut scripts = vec![NativePageScript::ImportMap(import_map.clone())];
    let mut resource_events = Vec::new();
    for (index, source) in sources.into_iter().enumerate() {
        match source {
            NativePageScriptSource::ImportMap {
                source,
                node_index,
                nonce,
            } => {
                let Some(source) = source else {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                };
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                let Ok(parsed) = NativeModuleImportMap::parse(&source, document_url) else {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                };
                let mut merged = import_map.clone();
                if merged.merge(parsed).is_err() {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                import_map = merged;
                scripts.push(NativePageScript::ImportMap(import_map.clone()));
            }
            NativePageScriptSource::Inline {
                source,
                node_index,
                nonce,
                ..
            } => {
                let allowed =
                    loader.allows_inline_script(document_url, &source, nonce.as_deref())?;
                if !allowed {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                scripts.push(NativePageScript::Classic {
                    source,
                    base_url: document_url.to_owned(),
                    node_index: Some(node_index),
                });
            }
            NativePageScriptSource::ModuleInline {
                source,
                timing,
                node_index,
                nonce,
                ..
            } => {
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                let name =
                    format!("{document_url}#glass-local-dynamic-module-{node_index}-{index}");
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
                    Ok(graph) => scripts.extend(graph.into_iter().map(|(_, script)| script)),
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
                scripts.push(NativePageScript::ImportMap(import_map.clone()));
            }
            NativePageScriptSource::External {
                href,
                node_index,
                nonce,
                integrity,
                parser_inserted,
                ..
            } => {
                let is_blob = href
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
                let is_file = is_file_subresource_source(document_url, &href);
                if !is_blob && !is_file {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason:
                            "dynamic external scripts require a rooted file or process-backed HTTP(S) document"
                                .into(),
                    });
                }
                let resource = if is_blob {
                    let object_url = runtime.object_url_resource(&href)?;
                    loader.load_local_blob_script(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        integrity.as_deref(),
                        object_url.as_ref(),
                    )
                } else {
                    loader.load_local_file_script_with_policy(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        parser_inserted,
                        nonce.as_deref(),
                        integrity.as_deref(),
                    )
                };
                match resource {
                    Ok(Some(resource)) => {
                        scripts.push(NativePageScript::Classic {
                            source: resource.body,
                            base_url: resource.url,
                            node_index: Some(node_index),
                        });
                        resource_events.push((node_index, NativeEventKind::Load));
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
            } => {
                let request_url = match resolve_module_request_url(document_url, &href) {
                    Ok(request_url) => request_url,
                    Err(_) => {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    }
                };
                let is_blob = href
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
                let is_file = is_file_subresource_source(document_url, &href);
                if !is_blob && !is_file {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason:
                            "dynamic external module scripts require a rooted file or process-backed HTTP(S) document"
                                .into(),
                    });
                }
                let resource = if is_blob {
                    let object_url = runtime.object_url_resource(&href)?;
                    loader.load_local_blob_script(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        integrity.as_deref(),
                        object_url.as_ref(),
                    )
                } else {
                    loader.load_local_file_script_with_policy(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        parser_inserted,
                        nonce.as_deref(),
                        integrity.as_deref(),
                    )
                };
                match resource {
                    Ok(Some(resource)) => {
                        let response_url = resource.url;
                        let source = resource.body;
                        let graph = if is_file {
                            load_local_file_module_graph_with_import_map(
                                NativeLocalModuleGraphRequest {
                                    loader,
                                    document_url,
                                    root_name: request_url,
                                    root_base_url: response_url,
                                    root_source: source,
                                    timing,
                                    node_index: Some(node_index),
                                    import_map: &mut import_map,
                                    existing_module_names: &BTreeSet::new(),
                                    existing_module_bytes: 0,
                                },
                            )
                        } else {
                            Ok(vec![(
                                timing,
                                NativePageScript::Module {
                                    name: request_url,
                                    source,
                                    base_url: response_url,
                                    referrer_policy: None,
                                    node_index: Some(node_index),
                                },
                            )])
                        };
                        match graph {
                            Ok(graph) => {
                                scripts.extend(graph.into_iter().map(|(_, script)| script));
                                resource_events.push((node_index, NativeEventKind::Load));
                            }
                            Err(_) => {
                                resource_events.push((node_index, NativeEventKind::Error));
                            }
                        }
                        scripts.push(NativePageScript::ImportMap(import_map.clone()));
                    }
                    Ok(None) | Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
        }
    }
    Ok((scripts, resource_events))
}

pub(super) fn execute_local_dynamic_page_script_batches(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let mut aggregate = NativePageScriptResult::default();
    let mut batches = 0usize;
    loop {
        batches = batches.saturating_add(1);
        if batches > super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native local dynamic script turns",
                super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                batches,
            ));
        }
        let (scripts, resource_events) =
            load_local_dynamic_page_script_sources(sources, runtime, loader, document_url)?;
        let inline_style_state_before =
            is_file_url(document_url).then(|| inline_style_source_state(document));
        runtime.set_sync_xhr_loader(loader);
        let mut result = execute_dynamic_page_scripts(NativeDynamicPageScriptRequest {
            document,
            runtime,
            sources: scripts,
            document_url,
            document_origin,
            viewport,
            resource_events: &resource_events,
            csp_violations: &[],
        })?;
        if is_file_url(document_url) {
            super::super::content_process::apply_pending_meta_content_security_policies(
                document,
                loader,
                document_url,
            )?;
            runtime.set_inline_script_policy(loader.inline_script_policy(document_url)?);
            runtime.mark_processed_inline_csp_meta_nodes(
                document.processed_content_security_policy_meta_nodes(),
            );
        }
        if let Some(inline_style_state_before) = inline_style_state_before {
            refresh_rooted_file_inline_styles(
                document,
                loader,
                document_url,
                Some(runtime),
                &inline_style_state_before,
            )?;
        }
        if let Some(updated_loader) = runtime.take_sync_xhr_loader() {
            loader.merge_fetch_task_state(updated_loader)?;
        }
        sources = std::mem::take(&mut result.pending_script_sources);
        merge_dynamic_page_script_result(&mut aggregate, result)?;
        if sources.is_empty() {
            return Ok(aggregate);
        }
    }
}

pub(super) fn inline_style_source_state(document: &NativeDocument) -> InlineStyleSourceState {
    (
        document.inline_style_elements(),
        document.inline_style_attributes(),
    )
}

pub(super) fn script_commands_may_change_inline_styles(commands: &[NativeScriptCommand]) -> bool {
    commands.iter().any(|command| match command {
        NativeScriptCommand::SetAttribute { name, .. }
        | NativeScriptCommand::RemoveAttribute { name, .. } => {
            name.eq_ignore_ascii_case("style") || name.eq_ignore_ascii_case("nonce")
        }
        NativeScriptCommand::SetTextContent { .. }
        | NativeScriptCommand::SetInnerHtml { .. }
        | NativeScriptCommand::AppendChild { .. }
        | NativeScriptCommand::InsertBefore { .. }
        | NativeScriptCommand::RemoveNode { .. } => true,
        _ => false,
    })
}

pub(super) fn refresh_rooted_file_inline_styles(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    runtime: Option<&NativeJavaScriptRuntime>,
    previous_state: &InlineStyleSourceState,
) -> Result<(), NativeEngineError> {
    if !is_file_url(document_url) {
        return Ok(());
    }
    let current_state = inline_style_source_state(document);
    if *previous_state == current_state {
        return Ok(());
    }
    super::super::content_process::refresh_inline_style_policy(document, loader, document_url)?;
    if previous_state.0 != current_state.0 {
        document.rebuild_external_stylesheet(document_url)?;
        load_font_faces(document, runtime, loader, document_url)?;
    }
    Ok(())
}

pub(super) fn load_local_dynamic_stylesheets(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let links = document
        .external_stylesheet_links_with_nonce()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_STYLESHEETS)
        .collect::<Vec<_>>();
    let previous_states = document.external_stylesheet_states();
    let live_local_nodes = links
        .iter()
        .filter(|(_, href, _, _, _)| {
            href.get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
                || is_file_subresource_source(document_url, href)
        })
        .map(|(node_index, _, _, _, _)| *node_index)
        .collect::<BTreeSet<_>>();
    let mut states = document
        .external_stylesheet_states()
        .into_iter()
        .map(|(node_index, href, stylesheet_url, body)| (node_index, (href, stylesheet_url, body)))
        .collect::<BTreeMap<_, _>>();
    states.retain(|node_index, _| live_local_nodes.contains(node_index));
    let mut loaded_bytes = states
        .values()
        .filter_map(|(_, _, body)| body.as_ref())
        .map(String::len)
        .sum::<usize>();
    let mut events = Vec::new();

    for (node_index, href, integrity, _, nonce) in links {
        let is_blob = href
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_file = is_file_subresource_source(document_url, &href);
        if !is_blob && !is_file {
            continue;
        }
        if states
            .get(&node_index)
            .is_some_and(|(loaded_href, _, _)| loaded_href == &href)
        {
            continue;
        }
        if let Some((_, _, previous_body)) = states.get(&node_index)
            && let Some(previous_body) = previous_body
        {
            loaded_bytes = loaded_bytes.saturating_sub(previous_body.len());
        }
        let body = if is_blob {
            let object_url = runtime.object_url_resource(&href)?;
            match loader.load_local_blob_stylesheet(
                document_url,
                &href,
                integrity.as_deref(),
                object_url.as_ref(),
            ) {
                Ok(Some(stylesheet)) => {
                    let next_len = loaded_bytes.saturating_add(stylesheet.len());
                    if next_len <= MAX_NATIVE_LOCAL_STYLESHEET_BYTES {
                        loaded_bytes = next_len;
                        Some(stylesheet)
                    } else {
                        None
                    }
                }
                Ok(None) | Err(_) => None,
            }
        } else {
            match loader.load_local_file_stylesheet_with_nonce(
                document_url,
                &href,
                integrity.as_deref(),
                nonce.as_deref(),
            ) {
                Ok(Some(stylesheet)) => expand_local_file_stylesheet_imports(
                    loader,
                    document_url,
                    &href,
                    stylesheet,
                    &mut loaded_bytes,
                    viewport,
                )
                .ok(),
                Ok(None) | Err(_) => None,
            }
        };
        let event_kind = body
            .as_ref()
            .map_or(NativeEventKind::Error, |_| NativeEventKind::Load);
        states.insert(node_index, (href, None, body));
        events.push((node_index, event_kind));
    }

    let next_states = states
        .into_iter()
        .map(|(node_index, (href, stylesheet_url, body))| (node_index, href, stylesheet_url, body))
        .collect::<Vec<_>>();
    if previous_states != next_states {
        document.set_external_stylesheet_states(next_states);
        document.rebuild_external_stylesheet(document_url)?;
        load_font_faces(document, Some(runtime), loader, document_url)?;
        document.refresh_background_image_sources();
    }
    Ok(events)
}

pub(super) fn load_local_dynamic_images(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document
        .external_image_links(viewport)
        .into_iter()
        .take(MAX_NATIVE_LOCAL_IMAGES)
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_blob && !is_file {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.image_resource_for_node(node_id).is_some() {
            continue;
        }
        document.mark_image_load(node_index, source.clone(), viewport)?;
        let image = if is_blob {
            let object_url = runtime.object_url_resource(&source)?;
            loader.load_local_blob_image(document_url, &source, object_url.as_ref())?
        } else {
            loader
                .load_local_file_image(document_url, &source)
                .unwrap_or_default()
        };
        let event_kind = match image {
            Some(image) => {
                document.set_image_resource(node_index, source, image)?;
                NativeEventKind::Load
            }
            None => NativeEventKind::Error,
        };
        events.push((node_index, event_kind));
    }
    document.refresh_background_image_sources();
    for (node_index, source) in document
        .external_background_image_links(MAX_NATIVE_LOCAL_IMAGES)
        .into_iter()
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_blob && !is_file {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document
            .background_image_resource_for_node(node_id)
            .is_some()
        {
            continue;
        }
        let image = if is_blob {
            let object_url = runtime.object_url_resource(&source)?;
            loader.load_local_blob_image(document_url, &source, object_url.as_ref())?
        } else {
            loader
                .load_local_file_image(document_url, &source)
                .unwrap_or_default()
        };
        if let Some(image) = image {
            document.set_background_image_resource(node_index, source, image)?;
        }
    }
    Ok(events)
}

pub(super) fn load_local_dynamic_blob_media(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document
        .external_media_links()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_MEDIA)
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_data = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_blob && !is_data && !is_file {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.has_media_resource_for_node(node_id) {
            continue;
        }
        document.mark_media_load(node_index, source.clone())?;
        let metadata = if is_blob {
            let object_url = runtime.object_url_resource(&source)?;
            loader.load_local_blob_media(document_url, &source, object_url.as_ref())?
        } else if is_file {
            loader.load_local_file_media(document_url, &source)?
        } else {
            loader.load_data_media(document_url, &source)?
        };
        let event_kind = match metadata {
            Some(metadata) => {
                document.set_media_resource(node_index, source, metadata)?;
                NativeEventKind::Load
            }
            None => {
                document.set_media_error(node_index, source)?;
                NativeEventKind::Error
            }
        };
        events.push((node_index, event_kind));
    }
    document.refresh_media_loads();
    Ok(events)
}

pub(super) fn load_local_initial_file_stylesheets(
    document: &NativeDocument,
    loader: &NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<InitialStylesheetState, NativeEngineError> {
    let mut states = Vec::new();
    let mut events = Vec::new();
    let mut loaded_bytes = 0usize;
    for (node_index, href, integrity, _, nonce) in document
        .external_stylesheet_links_with_nonce()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_STYLESHEETS)
    {
        let body = match loader.load_local_file_stylesheet_with_nonce(
            document_url,
            &href,
            integrity.as_deref(),
            nonce.as_deref(),
        ) {
            Ok(Some(stylesheet)) => expand_local_file_stylesheet_imports(
                loader,
                document_url,
                &href,
                stylesheet,
                &mut loaded_bytes,
                viewport,
            )
            .ok(),
            Ok(None) | Err(_) => None,
        };
        let event_kind = body
            .as_ref()
            .map_or(NativeEventKind::Error, |_| NativeEventKind::Load);
        states.push((node_index, href, None, body));
        events.push((node_index, event_kind));
    }
    Ok((states, events))
}

pub(super) fn expand_local_file_stylesheet_imports(
    loader: &NativeResourceLoader,
    document_url: &str,
    stylesheet_href: &str,
    stylesheet: String,
    loaded_bytes: &mut usize,
    viewport: Viewport,
) -> Result<String, NativeEngineError> {
    let stylesheet_url = resolve_local_file_stylesheet_url(document_url, stylesheet_href)?;
    let mut graph = NativeStylesheetImportGraph {
        entries: 0,
        bytes: 0,
        active: BTreeSet::new(),
        loaded: BTreeSet::from([stylesheet_url.clone()]),
        total_bytes: loaded_bytes,
    };
    let expanded = expand_local_file_stylesheet_body(
        loader,
        document_url,
        &stylesheet_url,
        stylesheet,
        &mut graph,
        viewport,
    )?;
    let next_bytes = graph.total_bytes.saturating_add(graph.bytes);
    if next_bytes > MAX_NATIVE_LOCAL_STYLESHEET_BYTES {
        return Err(NativeEngineError::limit(
            "native file stylesheet bytes",
            MAX_NATIVE_LOCAL_STYLESHEET_BYTES,
            next_bytes,
        ));
    }
    *graph.total_bytes = next_bytes;
    Ok(expanded)
}

fn expand_local_file_stylesheet_body(
    loader: &NativeResourceLoader,
    document_url: &str,
    stylesheet_url: &str,
    stylesheet: String,
    graph: &mut NativeStylesheetImportGraph<'_>,
    viewport: Viewport,
) -> Result<String, NativeEngineError> {
    graph.entries = graph.entries.saturating_add(1);
    if graph.entries > MAX_NATIVE_LOCAL_STYLESHEETS {
        return Err(NativeEngineError::limit(
            "native file stylesheet graph entries",
            MAX_NATIVE_LOCAL_STYLESHEETS,
            graph.entries,
        ));
    }
    graph.bytes = graph.bytes.saturating_add(stylesheet.len());
    if graph.bytes > MAX_NATIVE_LOCAL_STYLESHEET_BYTES {
        return Err(NativeEngineError::limit(
            "native file stylesheet graph bytes",
            MAX_NATIVE_LOCAL_STYLESHEET_BYTES,
            graph.bytes,
        ));
    }
    graph.active.insert(stylesheet_url.to_owned());
    let imports =
        static_css_imports(&stylesheet).map_err(|reason| NativeEngineError::UnsupportedUrl {
            reason: format!("file stylesheet import: {reason}"),
        })?;
    let mut expanded = String::with_capacity(stylesheet.len());
    let mut cursor = 0usize;
    for import in imports {
        expanded.push_str(&stylesheet[cursor..import.start]);
        if !css_import_matches(&import, viewport) {
            cursor = import.end;
            continue;
        }
        let target = resolve_local_file_stylesheet_url(stylesheet_url, &import.specifier)?;
        if graph.active.contains(&target) || !graph.loaded.insert(target.clone()) {
            cursor = import.end;
            continue;
        }
        let dependency = loader
            .load_local_file_stylesheet(document_url, &target, None)?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "file stylesheet dependency".into(),
                reason: format!(
                    "file stylesheet dependency {:?} was blocked or unavailable",
                    import.specifier
                ),
            })?;
        let dependency = expand_local_file_stylesheet_body(
            loader,
            document_url,
            &target,
            dependency,
            graph,
            viewport,
        )?;
        if let Some(layer) = import.layer {
            expanded.push_str("@layer ");
            expanded.push_str(&layer);
            expanded.push_str(" { ");
            expanded.push_str(&dependency);
            expanded.push_str(" }");
        } else {
            expanded.push_str(&dependency);
        }
        cursor = import.end;
    }
    expanded.push_str(&stylesheet[cursor..]);
    graph.active.remove(stylesheet_url);
    Ok(absolutize_stylesheet_urls(
        &expanded,
        document_url,
        stylesheet_url,
    ))
}

pub(super) fn resolve_local_file_stylesheet_url(
    owner_url: &str,
    href: &str,
) -> Result<String, NativeEngineError> {
    validate_url_text("file stylesheet import URL", href)?;
    let href = decode_css_url_value(href).ok_or_else(|| NativeEngineError::UnsupportedUrl {
        reason: "file stylesheet import URL contains an invalid CSS escape".into(),
    })?;
    let owner_url = url::Url::parse(without_fragment(owner_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet owner URL is not valid URL syntax".into(),
        }
    })?;
    if !owner_url.scheme().eq_ignore_ascii_case("file") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet owner URL must use the file scheme".into(),
        });
    }
    let mut target = owner_url
        .join(&href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet import URL could not be resolved against its owner".into(),
        })?;
    if !target.scheme().eq_ignore_ascii_case("file") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet imports must use the file scheme".into(),
        });
    }
    if !target.username().is_empty() || target.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet imports must not contain credentials".into(),
        });
    }
    if target
        .host_str()
        .is_some_and(|host| !host.eq_ignore_ascii_case("localhost"))
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet imports must use an empty or localhost host".into(),
        });
    }
    target.set_fragment(None);
    Ok(target.to_string())
}

pub(super) fn load_local_initial_file_images(
    document: &mut NativeDocument,
    loader: &NativeResourceLoader,
    document_url: &str,
    viewport: super::super::config::Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document
        .external_image_links(viewport)
        .into_iter()
        .take(MAX_NATIVE_LOCAL_IMAGES)
    {
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.image_resource_for_node(node_id).is_some() {
            continue;
        }
        document.mark_image_load(node_index, source.clone(), viewport)?;
        let image = if is_file_subresource_source(document_url, &source) {
            loader
                .load_local_file_image(document_url, &source)
                .unwrap_or_default()
        } else {
            None
        };
        let event_kind = match image {
            Some(image) => {
                document.set_image_resource(node_index, source, image)?;
                NativeEventKind::Load
            }
            None => NativeEventKind::Error,
        };
        events.push((node_index, event_kind));
    }
    document.refresh_background_image_sources();
    for (node_index, source) in document
        .external_background_image_links(MAX_NATIVE_LOCAL_IMAGES)
        .into_iter()
    {
        if !is_file_subresource_source(document_url, &source) {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document
            .background_image_resource_for_node(node_id)
            .is_some()
        {
            continue;
        }
        if let Some(image) = loader.load_local_file_image(document_url, &source)? {
            document.set_background_image_resource(node_index, source, image)?;
        }
    }
    Ok(events)
}

pub(super) fn load_local_initial_media(
    document: &mut NativeDocument,
    loader: &NativeResourceLoader,
    document_url: &str,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let media_base_url = document.effective_base_url(document_url);
    let mut events = Vec::new();
    for (node_index, source) in document.external_media_links() {
        let is_data = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"));
        let is_file = is_file_subresource_source(&media_base_url, &source);
        if !is_data && !is_file {
            continue;
        }
        document.mark_media_load(node_index, source.clone())?;
        let metadata = if is_file {
            loader.load_local_file_media(&media_base_url, &source)?
        } else {
            loader.load_data_media(&media_base_url, &source)?
        };
        let event_kind = match metadata {
            Some(metadata) => {
                document.set_media_resource(node_index, source, metadata)?;
                NativeEventKind::Load
            }
            None => {
                document.set_media_error(node_index, source)?;
                NativeEventKind::Error
            }
        };
        events.push((node_index, event_kind));
    }
    document.refresh_media_loads();
    Ok(events)
}

pub(super) fn is_file_subresource_source(document_url: &str, source: &str) -> bool {
    if source
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        return true;
    }
    let Ok(base) = url::Url::parse(without_fragment(document_url)) else {
        return false;
    };
    is_file_url(base.as_str())
        && base
            .join(source)
            .is_ok_and(|target| target.scheme().eq_ignore_ascii_case("file"))
}

pub(super) fn retain_page_script_fetch_commands(
    commands: &[NativeScriptCommand],
    pending_fetches: &mut Vec<NativeScriptCommand>,
) {
    pending_fetches.extend(
        commands
            .iter()
            .filter(|command| matches!(command, NativeScriptCommand::Fetch { .. }))
            .cloned(),
    );
}

pub(super) fn load_font_faces(
    document: &mut NativeDocument,
    runtime: Option<&NativeJavaScriptRuntime>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(), NativeEngineError> {
    let mut resources = Vec::new();
    let system_fonts = NativeFontBook::system();
    for rule in document
        .font_face_rules()
        .iter()
        .take(MAX_NATIVE_FONT_FACES)
    {
        for source in &rule.sources {
            let bytes = match source {
                NativeFontFaceSource::Local(family) => system_fonts.local_font_bytes(
                    family,
                    FontWeightValue::from_numeric(rule.weight.nominal())
                        .unwrap_or(FontWeightValue::Normal),
                    rule.style,
                ),
                NativeFontFaceSource::Url(source) => {
                    let object_url = runtime
                        .map(|runtime| runtime.object_url_resource(source))
                        .transpose()?
                        .flatten();
                    loader
                        .load_font(document_url, source, object_url.as_ref())
                        .ok()
                        .flatten()
                }
            };
            if let Some(bytes) = bytes {
                resources.push(NativeFontFaceResource::from_rule(rule, bytes));
                break;
            }
        }
    }
    document.set_font_resources(resources)
}

pub(super) fn extract_local_scroll_commands(
    commands: &[NativeScriptCommand],
) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| matches!(command, NativeScriptCommand::ScrollTo { .. }))
        .cloned()
        .collect()
}
