use super::*;

impl NativeEngine {
    pub(super) fn apply_local_evaluation_commands(
        &mut self,
        document: &mut NativeDocument,
        commands: &[NativeScriptCommand],
        history_commands: &mut Vec<NativeScriptCommand>,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        history_commands.extend(extract_local_history_commands(commands));
        let (effects, follow_up_commands) =
            self.apply_local_document_commands_with_font_face_ack(document, commands, false)?;
        history_commands.extend(extract_local_history_commands(&follow_up_commands));
        Ok(effects)
    }

    pub(super) fn apply_local_script_commands(
        &mut self,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if commands.is_empty() {
            return Ok(None);
        }

        let mut command_queue = VecDeque::from(commands.to_vec());
        let mut module_fetches = VecDeque::new();
        let mut navigation = None;
        let mut resolved_module_requests = 0usize;
        while !command_queue.is_empty() || !module_fetches.is_empty() {
            let mut batch = Vec::new();
            while let Some(command) = command_queue.pop_front() {
                match &command {
                    NativeScriptCommand::Fetch {
                        destination: Some(destination),
                        module_referrer: Some(_),
                        ..
                    } if destination == "module" => module_fetches.push_back(command),
                    NativeScriptCommand::Fetch { .. }
                    | NativeScriptCommand::WebSocketOpen { .. }
                    | NativeScriptCommand::WebSocketSend { .. }
                    | NativeScriptCommand::WebSocketClose { .. }
                    | NativeScriptCommand::EventSourceOpen { .. }
                    | NativeScriptCommand::EventSourceClose { .. }
                    | NativeScriptCommand::FetchStreamRead { .. }
                    | NativeScriptCommand::FetchStreamCancel { .. } => {
                        return Err(NativeEngineError::UnsupportedUrl {
                            reason: "script network transport requires a process-backed HTTP(S) document"
                                .into(),
                        });
                    }
                    _ => batch.push(command),
                }
            }

            if !batch.is_empty() {
                let mut follow_up_fetches = Vec::new();
                let batch_navigation = self.apply_local_script_command_batch(
                    &batch,
                    allow_script_navigation,
                    &mut follow_up_fetches,
                )?;
                module_fetches.extend(follow_up_fetches);
                if let Some(batch_navigation) = batch_navigation {
                    if navigation.is_some() {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "one script event-loop batch cannot activate multiple navigations"
                                    .into(),
                        });
                    }
                    navigation = Some(batch_navigation);
                }
            }

            let Some(module_fetch) = module_fetches.pop_front() else {
                continue;
            };
            resolved_module_requests = resolved_module_requests.saturating_add(1);
            if resolved_module_requests > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "rooted-file dynamic module requests",
                    MAX_NATIVE_MODULE_IMPORTS,
                    resolved_module_requests,
                ));
            }
            command_queue.extend(self.resolve_local_dynamic_module_fetch(module_fetch)?);
        }

        Ok(navigation)
    }

    pub(super) fn resolve_local_dynamic_module_fetch(
        &self,
        command: NativeScriptCommand,
    ) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
        let NativeScriptCommand::Fetch {
            request_id,
            worker_id,
            href,
            credentials,
            credentials_mode: _,
            referrer: _,
            referrer_url: _,
            referrer_policy: _,
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
            return Err(NativeEngineError::invalid(
                "rooted-file dynamic module request",
                "command was not a fetch request",
            ));
        };
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "rooted-file dynamic module request id",
                "must be positive",
            ));
        }
        let module_referrer = module_referrer.unwrap_or_default();
        let request_is_valid = worker_id.is_none()
            && credentials
            && method == "GET"
            && headers.is_empty()
            && body.is_none()
            && body_base64.is_none()
            && content_type.is_none()
            && mode.as_deref() == Some("cors")
            && redirect.as_deref() == Some("follow")
            && cache.as_deref() == Some("default")
            && timeout_ms.is_none()
            && upload_stream_id.is_none()
            && destination.as_deref() == Some("module")
            && module_type != Some(NativeModuleResourceType::Unsupported);

        let runtime = self
            .javascript
            .as_ref()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "rooted-file dynamic module import".into(),
                reason: "page JavaScript runtime is unavailable".into(),
            })?;
        let loaded_module = if request_is_valid {
            self.load_local_dynamic_page_module(
                request_id,
                &module_referrer,
                &href,
                module_type,
                runtime,
            )
        } else {
            Err(NativeEngineError::invalid(
                "rooted-file dynamic module request",
                "must be a credentialed bodyless CORS GET with default cache and follow redirects",
            ))
        };
        let module_alias = loaded_module.as_ref().ok().cloned();
        let payload = match loaded_module {
            Ok(module_key) => serde_json::json!({
                "dynamicModuleImport": {"moduleKey": module_key},
            }),
            Err(_) => serde_json::json!({
                "dynamicModuleImport": {"error": "Failed to load dynamically imported module"},
            }),
        };
        let evaluation = runtime.resolve_fetch(
            request_id,
            &payload,
            NativePageEventContext {
                document: &self.document,
                document_url: &self.url,
                origin: &self.origin,
                viewport: self.config.viewport,
                csp_violations: &[],
            },
            &NativePageEventBatch::default(),
        );
        if let Some(module_alias) = module_alias {
            runtime.discard_dynamic_module_alias(&module_alias);
        }
        let evaluation = evaluation?;
        if evaluation.top_level_await_pending {
            return Err(NativeEngineError::Worker {
                operation: "rooted-file dynamic module import".into(),
                reason: "module evaluation remained pending without a native host operation".into(),
            });
        }
        Ok(evaluation.commands)
    }

    pub(super) fn load_local_dynamic_page_module(
        &self,
        request_id: u32,
        module_referrer: &str,
        specifier: &str,
        module_type: Option<NativeModuleResourceType>,
        runtime: &NativeJavaScriptRuntime,
    ) -> Result<String, NativeEngineError> {
        if !is_file_url(&self.url) || !is_file_url(module_referrer) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason:
                    "rooted-file dynamic modules require a file document and active file referrer"
                        .into(),
            });
        }
        if !runtime.is_dynamic_import_referrer_allowed(module_referrer)? {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "dynamic module referrer is not an active page script".into(),
            });
        }

        let mut import_map = runtime.module_import_map()?;
        let target_result = import_map.resolve_and_record(module_referrer, specifier);
        runtime.set_module_import_map(import_map.clone());
        let target = target_result.map_err(|reason| NativeEngineError::Network {
            operation: "rooted-file dynamic module import".into(),
            reason: reason.into(),
        })?;
        if !is_file_url(&target) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "rooted-file dynamic module target must use the file scheme".into(),
            });
        }
        let module_type = module_type.unwrap_or(NativeModuleResourceType::JavaScript);
        if module_type == NativeModuleResourceType::Unsupported {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "rooted-file dynamic import attributes request an unsupported type".into(),
            });
        }
        let module_name = super::super::javascript::native_module_loader_name(&target, module_type);

        let (existing_sources, _) = runtime.module_sources_snapshot()?;
        if existing_sources.contains_key(&module_name) {
            return runtime.register_dynamic_module_alias(request_id, &target);
        }
        if existing_sources.len() >= MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "native page module graph entries",
                MAX_NATIVE_MODULE_IMPORTS,
                existing_sources.len().saturating_add(1),
            ));
        }
        let existing_names = existing_sources.keys().cloned().collect::<BTreeSet<_>>();
        let existing_bytes = existing_sources
            .values()
            .map(String::len)
            .fold(0usize, usize::saturating_add);
        let integrity = import_map.integrity_for_url(&target).map(str::to_owned);
        let resource = self
            .loader
            .load_local_file_script_with_policy(
                &self.url,
                &target,
                MAX_NATIVE_SCRIPT_BYTES,
                false,
                None,
                integrity.as_deref(),
            )?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "rooted-file dynamic module import".into(),
                reason: "module resource is missing, blocked, or outside the configured file root"
                    .into(),
            })?;
        let graph_result =
            load_local_file_dynamic_module_graph_with_import_map(NativeLocalModuleGraphRequest {
                loader: &self.loader,
                document_url: &self.url,
                root_name: module_name.clone(),
                root_base_url: resource.url,
                root_source: resource.body,
                timing: super::super::dom::NativePageScriptTiming::ParserBlocking,
                node_index: None,
                import_map: &mut import_map,
                existing_module_names: &existing_names,
                existing_module_bytes: existing_bytes,
            });
        runtime.set_module_import_map(import_map);
        let graph = graph_result?;

        let mut new_sources = BTreeMap::new();
        let mut new_base_urls = BTreeMap::new();
        let mut new_referrer_policies = BTreeMap::new();
        for (_, script) in graph {
            let (name, source, base_url, referrer_policy) = match script {
                NativePageScript::Module {
                    name,
                    source,
                    base_url,
                    referrer_policy,
                    ..
                } => (name, source, base_url, referrer_policy),
                NativePageScript::ModuleDependency {
                    name,
                    source,
                    base_url,
                    referrer_policy,
                } => (name, source, base_url, referrer_policy),
                NativePageScript::Classic { .. } | NativePageScript::ImportMap(_) => continue,
            };
            new_sources.entry(name.clone()).or_insert(source);
            new_base_urls.entry(name.clone()).or_insert(base_url);
            if let Some(referrer_policy) = referrer_policy {
                new_referrer_policies.entry(name).or_insert(referrer_policy);
            }
        }
        let combined_entries = existing_sources.len().saturating_add(new_sources.len());
        if combined_entries > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "native page module graph entries",
                MAX_NATIVE_MODULE_IMPORTS,
                combined_entries,
            ));
        }
        let combined_bytes = existing_bytes.saturating_add(
            new_sources
                .values()
                .map(String::len)
                .fold(0usize, usize::saturating_add),
        );
        let maximum_module_bytes =
            MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS);
        if combined_bytes > maximum_module_bytes {
            return Err(NativeEngineError::limit(
                "native page module graph bytes",
                maximum_module_bytes,
                combined_bytes,
            ));
        }
        runtime.extend_module_sources(new_sources, new_base_urls)?;
        runtime.extend_module_referrer_policies(new_referrer_policies)?;
        runtime.register_dynamic_module_alias(request_id, &target)
    }

    pub(super) fn apply_local_script_command_batch(
        &mut self,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
        pending_fetches: &mut Vec<NativeScriptCommand>,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if commands.is_empty() {
            return Ok(None);
        }
        if commands.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::Fetch { .. }
                    | NativeScriptCommand::WebSocketOpen { .. }
                    | NativeScriptCommand::WebSocketSend { .. }
                    | NativeScriptCommand::WebSocketClose { .. }
                    | NativeScriptCommand::EventSourceOpen { .. }
                    | NativeScriptCommand::EventSourceClose { .. }
                    | NativeScriptCommand::FetchStreamRead { .. }
                    | NativeScriptCommand::FetchStreamCancel { .. }
            )
        }) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "script network transport requires a process-backed HTTP(S) document"
                    .into(),
            });
        }

        let mut effective_commands = commands.to_vec();
        let mut document = self.document.clone();
        let (mut events, font_face_follow_up_commands) = self
            .apply_local_document_commands_with_font_face_ack(
                &mut document,
                commands,
                allow_script_navigation,
            )?;
        effective_commands.extend(font_face_follow_up_commands.iter().cloned());
        let mut history_commands = self.prepare_local_history_commands(&effective_commands)?;
        let mut scroll_commands = extract_local_scroll_commands(&effective_commands);
        if font_face_follow_up_commands.iter().any(|command| {
            matches!(
                command,
                super::super::javascript::NativeScriptCommand::Fetch { .. }
                    | super::super::javascript::NativeScriptCommand::WebSocketOpen { .. }
                    | super::super::javascript::NativeScriptCommand::WebSocketSend { .. }
                    | super::super::javascript::NativeScriptCommand::WebSocketClose { .. }
                    | super::super::javascript::NativeScriptCommand::EventSourceOpen { .. }
                    | super::super::javascript::NativeScriptCommand::EventSourceClose { .. }
                    | super::super::javascript::NativeScriptCommand::FetchStreamRead { .. }
                    | super::super::javascript::NativeScriptCommand::FetchStreamCancel { .. }
            )
        }) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "script network transport requires a process-backed HTTP(S) document"
                    .into(),
            });
        }
        let mut dynamic_navigation = None;
        let mut dynamic_dialogs = Vec::new();
        if let Some(javascript) = self.javascript.as_ref() {
            let dynamic_sources = document.take_newly_attached_page_script_sources(
                &effective_commands,
                super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                MAX_NATIVE_SCRIPT_BYTES,
            );
            if !dynamic_sources.is_empty() {
                let dynamic_result = execute_local_dynamic_page_script_batches(
                    &mut document,
                    javascript,
                    dynamic_sources,
                    &mut self.loader,
                    &self.url,
                    &self.origin,
                    self.config.viewport,
                )?;
                if !dynamic_result.pending_script_sources.is_empty() {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "dynamic external/module scripts require a process-backed HTTP(S) document"
                            .into(),
                    });
                }
                pending_fetches.extend(dynamic_result.pending_fetches);
                if !dynamic_result.websocket_commands.is_empty()
                    || !dynamic_result.event_source_commands.is_empty()
                {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "dynamic script network transport requires a process-backed HTTP(S) document"
                            .into(),
                    });
                }
                events.extend(dynamic_result.events.into_iter().map(|(node_index, kind)| {
                    (
                        NativeNodeId::from_parts(document.generation(), node_index),
                        kind,
                    )
                }));
                scroll_commands.extend(dynamic_result.scroll_commands);
                if let Some(navigation) = dynamic_result.navigation {
                    dynamic_navigation = Some(ScriptNavigationTarget::Location {
                        href: navigation.href,
                        replace_history: navigation.replace_history,
                    });
                }
                dynamic_dialogs = dynamic_result.dialogs;
            }
        }
        if !dynamic_dialogs.is_empty() {
            let dialog_url = self.url.clone();
            self.install_dialogs(dynamic_dialogs, &dialog_url)?;
        }
        let (stylesheet_events, image_events, media_events) =
            if let Some(javascript) = self.javascript.as_ref() {
                let stylesheet_events = load_local_dynamic_stylesheets(
                    &mut document,
                    javascript,
                    &mut self.loader,
                    &self.url,
                    self.config.viewport,
                )?;
                let image_events = load_local_dynamic_images(
                    &mut document,
                    javascript,
                    &mut self.loader,
                    &self.url,
                    self.config.viewport,
                )?;
                let media_events = load_local_dynamic_blob_media(
                    &mut document,
                    javascript,
                    &mut self.loader,
                    &self.url,
                )?;
                (stylesheet_events, image_events, media_events)
            } else {
                (Vec::new(), Vec::new(), Vec::new())
            };
        for (node_index, event_kind) in stylesheet_events {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(node_id, event_kind)])?
            {
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
            }
            events.push((node_id, event_kind));
        }
        for (node_index, event_kind) in image_events {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(node_id, event_kind)])?
            {
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
            }
            events.push((node_id, event_kind));
        }
        for (node_index, event_kind) in media_events {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(node_id, event_kind)])?
            {
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
            }
            events.push((node_id, event_kind));
        }
        let validation_events = events
            .iter()
            .filter(|(_, kind)| *kind == NativeEventKind::Invalid)
            .copied()
            .collect::<Vec<_>>();
        if !validation_events.is_empty()
            && let Some(evaluation) = self.evaluate_local_events(&document, &validation_events)?
        {
            retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
            let (effects, follow_up_commands) = self
                .apply_local_document_commands_with_font_face_ack(
                    &mut document,
                    &evaluation.commands,
                    false,
                )?;
            events.extend(effects);
            self.retain_local_font_face_follow_up_commands(
                &mut effective_commands,
                &mut scroll_commands,
                &mut history_commands,
                follow_up_commands,
            )?;
        }
        let mut navigation = if allow_script_navigation {
            self.script_navigation_target(&document, &effective_commands)?
        } else {
            None
        };
        if let Some(dynamic_navigation) = dynamic_navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one script batch cannot activate multiple navigations".into(),
                });
            }
            navigation = Some(dynamic_navigation);
        }
        if let Some(ScriptNavigationTarget::Form {
            form_id,
            dispatch_submit: true,
            submitter,
            ..
        }) = navigation.as_ref()
        {
            let invalid = document.invalid_form_controls(*form_id, *submitter)?;
            if invalid.is_empty() {
                let evaluation = self
                    .evaluate_local_submit_event(&document, *form_id, *submitter)?
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "native submit event".into(),
                        reason: "native JavaScript realm disappeared during submit dispatch".into(),
                    })?;
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let allowed = evaluation
                    .value
                    .as_array()
                    .and_then(|values| values.first())
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "native submit event".into(),
                        reason: "native submit event result was invalid".into(),
                    })?;
                events.push((*form_id, NativeEventKind::Submit));
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
                if !allowed {
                    navigation = None;
                }
            } else {
                let invalid_events = invalid
                    .iter()
                    .copied()
                    .map(|id| (id, NativeEventKind::Invalid))
                    .collect::<Vec<_>>();
                if let Some(evaluation) = self.evaluate_local_events(&document, &invalid_events)? {
                    retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                    events.extend(invalid_events);
                    let (effects, follow_up_commands) = self
                        .apply_local_document_commands_with_font_face_ack(
                            &mut document,
                            &evaluation.commands,
                            false,
                        )?;
                    events.extend(effects);
                    self.retain_local_font_face_follow_up_commands(
                        &mut effective_commands,
                        &mut scroll_commands,
                        &mut history_commands,
                        follow_up_commands,
                    )?;
                }
                navigation = None;
            }
        }
        if history_commands
            .iter()
            .any(|command| matches!(command, LocalHistoryCommand::Go(_)))
            && navigation.is_some()
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share a script batch with navigation".into(),
            });
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        let scroll_events = self.apply_scroll_commands(&self.document.clone(), &scroll_commands)?;
        events.extend(scroll_events.iter().copied());
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_prepared_history_commands(history_commands, next_revision)?;
        self.record_effects(events);
        if !scroll_events.is_empty() {
            self.dispatch_local_events(&scroll_events)?;
        }
        let navigation = match navigation {
            Some(ScriptNavigationTarget::Link { href, popup }) => {
                let target_url = self.resolve_link_href(&href)?;
                if popup {
                    self.queue_popup(target_url)?;
                    None
                } else {
                    let mut request = NativeNavigationRequest::get(target_url.clone());
                    request.object_url = self
                        .javascript
                        .as_ref()
                        .map(|javascript| javascript.object_url_resource(&target_url))
                        .transpose()?
                        .flatten();
                    Some(request)
                }
            }
            Some(ScriptNavigationTarget::Form {
                form_id,
                submitter,
                target,
                ..
            }) => {
                let mut request = self
                    .document
                    .form_submission_request_with_submitter(form_id, &self.url, submitter)?;
                request.target = Some(target);
                request.object_url = self
                    .javascript
                    .as_ref()
                    .map(|javascript| javascript.object_url_resource(&request.url))
                    .transpose()?
                    .flatten();
                self.loader
                    .allows_navigation(
                        &self.url,
                        &request.url,
                        NativeNavigationPolicyKind::FormAction,
                    )?
                    .then_some(request)
            }
            Some(ScriptNavigationTarget::Location {
                href,
                replace_history,
            }) => {
                let mut request = NativeNavigationRequest::get(self.resolve_link_href(&href)?);
                request.replace_history = replace_history;
                request.object_url = self
                    .javascript
                    .as_ref()
                    .map(|javascript| javascript.object_url_resource(&request.url))
                    .transpose()?
                    .flatten();
                Some(request)
            }
            None => None,
        };
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    pub(super) fn prepare_local_history_commands(
        &self,
        commands: &[super::super::javascript::NativeScriptCommand],
    ) -> Result<Vec<LocalHistoryCommand>, NativeEngineError> {
        let mut prepared = Vec::new();
        let mut base_url = self.url.clone();
        let mut traversal_seen = false;
        for command in commands {
            let prepared_command = match command {
                super::super::javascript::NativeScriptCommand::HistoryPushState { href, state } => {
                    if traversal_seen {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "history state mutation cannot follow traversal in one script batch"
                                    .into(),
                        });
                    }
                    let url = self.resolve_history_href_from(&base_url, href)?;
                    validate_history_state(state)?;
                    base_url = url.clone();
                    LocalHistoryCommand::PushState {
                        url,
                        state: state.clone(),
                    }
                }
                super::super::javascript::NativeScriptCommand::HistoryReplaceState {
                    href,
                    state,
                } => {
                    if traversal_seen {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "history state mutation cannot follow traversal in one script batch"
                                    .into(),
                        });
                    }
                    let url = self.resolve_history_href_from(&base_url, href)?;
                    validate_history_state(state)?;
                    base_url = url.clone();
                    LocalHistoryCommand::ReplaceState {
                        url,
                        state: state.clone(),
                    }
                }
                super::super::javascript::NativeScriptCommand::HistoryGo { delta } => {
                    if traversal_seen {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "a script batch cannot request multiple history traversals"
                                .into(),
                        });
                    }
                    traversal_seen = true;
                    LocalHistoryCommand::Go(*delta)
                }
                _ => continue,
            };
            prepared.push(prepared_command);
        }
        Ok(prepared)
    }

    pub(super) fn script_navigation_target(
        &self,
        document: &NativeDocument,
        commands: &[super::super::javascript::NativeScriptCommand],
    ) -> Result<Option<ScriptNavigationTarget>, NativeEngineError> {
        let mut navigation = None;
        for command in commands {
            let target = match command {
                super::super::javascript::NativeScriptCommand::Click { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    if let Some(href) = document.link_href(id).filter(|href| !href.is_empty()) {
                        Some(ScriptNavigationTarget::Link {
                            href: href.to_owned(),
                            popup: document.link_opens_new_target(id),
                        })
                    } else if let Some(form_id) = document.submit_control_form(id) {
                        let target = document.form_submission_target(form_id, Some(id))?;
                        document.form_submission_request_with_submitter(
                            form_id,
                            &self.url,
                            Some(id),
                        )?;
                        Some(ScriptNavigationTarget::Form {
                            form_id,
                            dispatch_submit: true,
                            submitter: Some(id),
                            target,
                        })
                    } else {
                        None
                    }
                }
                super::super::javascript::NativeScriptCommand::SubmitForm { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    let target = document.form_submission_target(id, None)?;
                    document.form_submission_request(id, &self.url)?;
                    Some(ScriptNavigationTarget::Form {
                        form_id: id,
                        dispatch_submit: false,
                        submitter: None,
                        target,
                    })
                }
                super::super::javascript::NativeScriptCommand::RequestSubmitForm {
                    node_index,
                    submitter_index,
                } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    let submitter = submitter_index
                        .map(|index| NativeNodeId::from_parts(document.generation(), index));
                    if let Some(submitter) = submitter
                        && document.submit_control_form(submitter) != Some(id)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "requestSubmit submitter must be a submit control for the form"
                                .into(),
                        });
                    }
                    let target = document.form_submission_target(id, submitter)?;
                    document.form_submission_request_with_submitter(id, &self.url, submitter)?;
                    Some(ScriptNavigationTarget::Form {
                        form_id: id,
                        dispatch_submit: true,
                        submitter,
                        target,
                    })
                }
                super::super::javascript::NativeScriptCommand::Navigate { href, replace } => {
                    validate_url_text("script location href", href)?;
                    Some(ScriptNavigationTarget::Location {
                        href: href.to_owned(),
                        replace_history: *replace,
                    })
                }
                _ => None,
            };
            let Some(target) = target else {
                continue;
            };
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one script batch cannot activate multiple navigations".into(),
                });
            }
            navigation = Some(target);
        }
        Ok(navigation)
    }

    pub(super) fn script_location_navigation_request(
        &self,
        commands: &[super::super::javascript::NativeScriptCommand],
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let mut navigation = None;
        for command in commands {
            let super::super::javascript::NativeScriptCommand::Navigate { href, replace } = command
            else {
                continue;
            };
            validate_url_text("script location href", href)?;
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one lifecycle event cannot activate multiple navigations".into(),
                });
            }
            let mut request = NativeNavigationRequest::get(self.resolve_link_href(href)?);
            request.replace_history = *replace;
            navigation = Some(request);
        }
        Ok(navigation)
    }

    pub(super) async fn navigate_script_navigation_async(
        &mut self,
        navigation: NativeContentNavigation,
        page_navigation_handoffs: usize,
    ) -> Result<(), NativeEngineError> {
        if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
            return Err(NativeEngineError::limit(
                "page navigation handoffs",
                MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                page_navigation_handoffs,
            ));
        }
        if navigation.location {
            let target_url = self.resolve_link_href(&navigation.href)?;
            let mut request = NativeNavigationRequest::get(target_url);
            request.replace_history = navigation.replace_history;
            Box::pin(self.navigate_request_async(request, page_navigation_handoffs))
                .await
                .map(|_| ())?;
            return Ok(());
        }
        let id = NativeNodeId::from_parts(self.document.generation(), navigation.node_index);
        let submitter = navigation
            .submitter_node_index
            .map(|index| NativeNodeId::from_parts(self.document.generation(), index));
        let download_attribute = self.document.link_download_attribute(id).map(str::to_owned);
        let mut request =
            if let Some(href) = self.document.link_href(id).filter(|href| !href.is_empty()) {
                NativeNavigationRequest::get(href)
            } else {
                self.document
                    .form_submission_request_with_submitter(id, &self.url, submitter)?
            };
        if request.url != navigation.href {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script navigation target changed during transfer".into(),
            });
        }
        if request.method != navigation.method
            || request.body != navigation.body
            || request.body_content_type != navigation.body_content_type
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script form request payload changed during transfer".into(),
            });
        }
        let request_target = request.target.as_deref().unwrap_or("_self");
        if !request_target.eq_ignore_ascii_case(&navigation.target) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script form target changed during transfer".into(),
            });
        }
        let target_url = self.resolve_link_href(&request.url)?;
        let same_document = self.is_same_document_navigation(&target_url);
        let target_kind = request_target.to_ascii_lowercase();
        let targets_current_document = match target_kind.as_str() {
            "_self" => true,
            "_parent" | "_top" | "_unfencedtop" => self.frame_script_context.is_none(),
            _ => false,
        };
        let opens_new_target = self.document.link_opens_new_target(id);
        if (same_document || download_attribute.is_some() || opens_new_target)
            && !self
                .allows_top_level_navigation_async(&target_url, true)
                .await?
        {
            return Ok(());
        }
        if let Some(download_attribute) = download_attribute {
            self.queue_download(target_url, &download_attribute)?;
            return Ok(());
        }
        if opens_new_target {
            self.queue_popup(target_url)?;
            return Ok(());
        }
        request.url = target_url.clone();
        request.replace_history = navigation.replace_history;
        if request.method == NativeNavigationMethod::Get
            && same_document
            && targets_current_document
        {
            let history_commit = if request.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    target_url,
                    history_commit,
                    &worker,
                    page_navigation_handoffs,
                )
                .await?;
            } else {
                if let Some(navigation) =
                    self.commit_same_document_navigation(target_url, history_commit)?
                {
                    self.navigate_page_script_sync(navigation, page_navigation_handoffs + 1)?;
                }
            }
            return Ok(());
        }
        Box::pin(self.navigate_request_async(request, page_navigation_handoffs))
            .await
            .map(|_| ())
    }
}
