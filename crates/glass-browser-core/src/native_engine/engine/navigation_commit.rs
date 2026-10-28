use super::*;

impl NativeEngine {
    pub(super) fn prepare_navigation(
        &mut self,
        url: &str,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load(url)?;
        self.prepare_navigation_resource(resource)
    }

    pub(super) async fn prepare_navigation_async(
        &mut self,
        url: &str,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load_async(url).await?;
        self.prepare_navigation_resource(resource)
    }

    pub(super) fn prepare_navigation_content(
        &self,
        content: NativeContentLoad,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        let generation = u32::try_from(next_revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let initial_events = content
            .events
            .iter()
            .map(|event| (event.node_index, event.kind))
            .collect();
        let mut document =
            NativeDocument::from_content_wire(content.document, &self.config.limits, generation)?;
        document.set_css_target_from_url(&content.url)?;
        document.set_viewport(self.config.viewport)?;
        Ok(PreparedNavigation {
            resource: NativeResource {
                url: content.url,
                origin: content.origin,
                body: String::new(),
            },
            document,
            frame_sources: content.frame_sources,
            navigate_to_sources: content.navigate_to_sources,
            dialogs: content.dialogs,
            initial_events,
            initial_scroll_commands: content.scroll_commands,
            execute_inline_scripts: false,
        })
    }

    pub(super) fn prepare_navigation_resource(
        &mut self,
        resource: NativeResource,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        self.prepare_navigation_resource_at_revision(resource, next_revision)
    }

    pub(super) fn prepare_navigation_resource_at_revision(
        &mut self,
        resource: NativeResource,
        revision: u64,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let generation = u32::try_from(revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let mut document = NativeDocument::parse_with_generation_and_referrer_policy(
            &resource.body,
            &self.config.limits,
            generation,
            self.loader.document_referrer_policy(&resource.url)?,
        )?;
        document.set_inherited_base_url(self.embedding_base_url.clone());
        document.set_viewport(self.config.viewport)?;
        let initial_events = if is_file_url(&resource.url) {
            self.loader.apply_meta_content_security_policies(
                &resource.url,
                &document.content_security_policy_meta(),
            )?;
            document.mark_content_security_policy_meta_processed();
            let allowed_inline_style_nodes =
                super::super::content_process::inline_style_policy_nodes(
                    &mut document,
                    &mut self.loader,
                    &resource.url,
                )?;
            let (stylesheet_states, mut events) = load_local_initial_file_stylesheets(
                &document,
                &self.loader,
                &resource.url,
                self.config.viewport,
            )?;
            let external_stylesheets = stylesheet_states
                .iter()
                .filter_map(|(_, href, stylesheet_url, body)| {
                    body.as_ref().map(|body| {
                        let stylesheet_url = stylesheet_url.as_deref().unwrap_or(href);
                        absolutize_stylesheet_urls(body, &resource.url, stylesheet_url)
                    })
                })
                .collect::<Vec<_>>();
            document = NativeDocument::parse_with_stylesheets_inline_policy_and_referrer_policy(
                &resource.body,
                &self.config.limits,
                &external_stylesheets,
                generation,
                Some(&allowed_inline_style_nodes),
                self.loader.document_referrer_policy(&resource.url)?,
            )?;
            document.set_inherited_base_url(self.embedding_base_url.clone());
            document.mark_inline_style_reports_seen();
            document.mark_content_security_policy_meta_processed();
            document.set_external_stylesheet_states(stylesheet_states);
            events.extend(load_local_initial_file_images(
                &mut document,
                &self.loader,
                &resource.url,
                self.config.viewport,
            )?);
            events.extend(load_local_initial_media(
                &mut document,
                &self.loader,
                &resource.url,
            )?);
            events
        } else {
            load_local_initial_media(&mut document, &self.loader, &resource.url)?
        };
        document.set_css_target_from_url(&resource.url)?;
        load_font_faces(&mut document, None, &mut self.loader, &resource.url)?;
        let frame_sources = self.loader.frame_sources_for_document(&resource.url)?;
        let navigate_to_sources = self.loader.navigation_sources_for_document(
            &resource.url,
            NativeNavigationPolicyKind::NavigateTo,
        )?;
        Ok(PreparedNavigation {
            resource,
            document,
            frame_sources,
            navigate_to_sources,
            dialogs: Vec::new(),
            initial_events,
            initial_scroll_commands: Vec::new(),
            execute_inline_scripts: true,
        })
    }

    pub(super) fn commit_navigation(
        &mut self,
        mut prepared: PreparedNavigation,
        history_commit: HistoryCommit,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let skip_lifecycle = std::mem::take(&mut self.skip_next_navigation_lifecycle);
        let mut initial_events = std::mem::take(&mut prepared.initial_events);
        let mut initial_scroll_commands = std::mem::take(&mut prepared.initial_scroll_commands);
        if !skip_lifecycle && self.javascript.is_some() {
            let (allowed, before_navigation) = self.dispatch_local_before_unload()?;
            if !allowed && self.document_has_sticky_activation {
                return Err(Self::synchronous_beforeunload_error());
            }
            let lifecycle_navigation = self.dispatch_local_navigation_lifecycle()?;
            if before_navigation.is_some() && lifecycle_navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple outgoing lifecycle navigations are not supported".into(),
                });
            }
            if let Some(navigation) = before_navigation.or(lifecycle_navigation) {
                self.skip_next_navigation_lifecycle = true;
                return Ok(Some(navigation));
            }
        }
        self.persist_local_web_storage()?;
        let storage_state = self.web_storage.clone();
        let cookie = self.loader.document_cookie(&prepared.resource.url)?;
        let mut javascript = if prepared.execute_inline_scripts {
            Some(NativeJavaScriptRuntime::new_with_context_metadata(
                &self.config.context_id,
                &self.config.window_name,
                self.config.opener_context_id.as_deref(),
                &self.config.opener_window_name,
                &self.config.opener_url,
            )?)
        } else {
            None
        };
        if let Some(javascript) = javascript.as_ref() {
            javascript.set_frame_script_bindings(self.frame_script_bindings.clone());
            javascript.set_frame_script_context(self.frame_script_context.clone());
            javascript.set_frame_id(self.frame_id.clone());
            javascript.set_environment(self.environment.clone());
        }
        let mut dialogs = std::mem::take(&mut prepared.dialogs);
        let page_navigation = if prepared.execute_inline_scripts {
            if let Some(javascript) = javascript.as_ref() {
                javascript.set_sync_xhr_loader(&self.loader);
            }
            let result = execute_inline_scripts(NativeInlinePageScriptRequest {
                document: &mut prepared.document,
                runtime: &mut javascript,
                context_id: &self.config.context_id,
                document_url: &prepared.resource.url,
                document_origin: &prepared.resource.origin,
                viewport: self.config.viewport,
                storage_state: &storage_state,
                indexed_db_state: &self.indexed_db,
                cookie: &cookie,
                loader: &mut self.loader,
            });
            if let Some(javascript) = javascript.as_ref()
                && let Some(updated_loader) = javascript.take_sync_xhr_loader()
            {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            let result = result?;
            if is_file_url(&prepared.resource.url) {
                super::super::content_process::apply_pending_meta_content_security_policies(
                    &mut prepared.document,
                    &mut self.loader,
                    &prepared.resource.url,
                )?;
                if let Some(runtime) = javascript.as_ref() {
                    self.synchronize_local_inline_csp_policy(
                        &prepared.document,
                        runtime,
                        &prepared.resource.url,
                    )?;
                }
            }
            dialogs.extend(result.dialogs);
            initial_events.extend(result.events);
            initial_scroll_commands.extend(result.scroll_commands);
            let popups = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_popup_events)
                .unwrap_or_default();
            self.queue_popup_requests(popups)?;
            let messages = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_post_message_events)
                .unwrap_or_default();
            self.queue_post_message_requests_from_origin(messages, &prepared.resource.origin)?;
            let window_closes = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_close_events)
                .unwrap_or_default();
            self.queue_window_close_requests(window_closes)?;
            let window_navigations = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_navigation_events)
                .unwrap_or_default();
            self.queue_window_navigation_requests(window_navigations)?;
            if let Some(javascript) = javascript.as_ref() {
                self.config.window_name = javascript.window_name();
            }
            result.navigation
        } else {
            None
        };
        let fragment_scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "navigation")?;
        let revision = prepared.document.revision();
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.workers.clear();
        self.pending_message_port_messages.clear();
        self.javascript = javascript;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
        self.embedding_base_url = None;
        self.scroll_offset = fragment_scroll_offset;
        self.sync_javascript_scroll_offset();
        let _ = self.apply_scroll_commands(&self.document.clone(), &initial_scroll_commands)?;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        match history_commit {
            HistoryCommit::Push => {
                self.history
                    .push(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Replace => {
                self.history
                    .replace_current(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Activate(_) => {
                return Err(NativeEngineError::Scheduler {
                    reason: "full navigation cannot activate a history entry".into(),
                });
            }
        }
        self.sync_javascript_history();
        self.flush_pending_lifecycle_effects();
        self.record_initial_events(initial_events)?;
        if let Some(javascript) = self.javascript.as_mut() {
            javascript.reset_timer_clock();
        }
        let page_navigation = match page_navigation {
            Some(page_navigation) => Some(self.page_navigation_request(page_navigation)?),
            None => self.dispatch_local_page_show()?,
        };
        if let Some(javascript) = self.javascript.as_mut() {
            javascript.reset_timer_clock();
        }
        self.persist_local_web_storage()?;
        Ok(page_navigation)
    }

    pub(super) async fn commit_navigation_async(
        &mut self,
        mut prepared: PreparedNavigation,
        worker: &NativeRuntimeWorker,
        history_commit: HistoryCommit,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if let HistoryCommit::Activate(history_index) = &history_commit {
            self.commit_history_navigation_async(prepared, *history_index, worker)
                .await?;
            return Ok(None);
        }
        let execute_page_scripts = prepared.execute_inline_scripts;
        let mut initial_events = std::mem::take(&mut prepared.initial_events);
        let mut initial_scroll_commands = std::mem::take(&mut prepared.initial_scroll_commands);
        self.persist_local_web_storage()?;
        let storage_state = self.web_storage.clone();
        let cookie = self.loader.document_cookie(&prepared.resource.url)?;
        let mut javascript = if execute_page_scripts {
            Some(NativeJavaScriptRuntime::new_with_context_metadata(
                &self.config.context_id,
                &self.config.window_name,
                self.config.opener_context_id.as_deref(),
                &self.config.opener_window_name,
                &self.config.opener_url,
            )?)
        } else {
            None
        };
        if let Some(javascript) = javascript.as_ref() {
            javascript.set_frame_script_bindings(self.frame_script_bindings.clone());
            javascript.set_frame_script_context(self.frame_script_context.clone());
            javascript.set_frame_id(self.frame_id.clone());
            javascript.set_environment(self.environment.clone());
        }
        let mut dialogs = std::mem::take(&mut prepared.dialogs);
        let mut initial_module_fetches = Vec::new();
        let page_navigation = if execute_page_scripts {
            if let Some(javascript) = javascript.as_ref() {
                javascript.set_sync_xhr_loader(&self.loader);
            }
            let inline_style_state_before = is_file_url(&prepared.resource.url)
                .then(|| inline_style_source_state(&prepared.document));
            let result = execute_inline_scripts(NativeInlinePageScriptRequest {
                document: &mut prepared.document,
                runtime: &mut javascript,
                context_id: &self.config.context_id,
                document_url: &prepared.resource.url,
                document_origin: &prepared.resource.origin,
                viewport: self.config.viewport,
                storage_state: &storage_state,
                indexed_db_state: &self.indexed_db,
                cookie: &cookie,
                loader: &mut self.loader,
            });
            if let Some(javascript) = javascript.as_ref()
                && let Some(updated_loader) = javascript.take_sync_xhr_loader()
            {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            let result = result?;
            if is_file_url(&prepared.resource.url) {
                super::super::content_process::apply_pending_meta_content_security_policies(
                    &mut prepared.document,
                    &mut self.loader,
                    &prepared.resource.url,
                )?;
                if let Some(runtime) = javascript.as_ref() {
                    self.synchronize_local_inline_csp_policy(
                        &prepared.document,
                        runtime,
                        &prepared.resource.url,
                    )?;
                }
            }
            if let Some(inline_style_state_before) = inline_style_state_before
                && inline_style_state_before != inline_style_source_state(&prepared.document)
            {
                refresh_rooted_file_inline_styles(
                    &mut prepared.document,
                    &mut self.loader,
                    &prepared.resource.url,
                    javascript.as_ref(),
                    &inline_style_state_before,
                )?;
            }
            initial_module_fetches.extend(result.pending_fetches.into_iter().filter(|command| {
                matches!(
                    command,
                    NativeScriptCommand::Fetch {
                        destination: Some(destination),
                        module_referrer: Some(_),
                        ..
                    } if destination == "module"
                )
            }));
            dialogs.extend(result.dialogs);
            initial_events.extend(result.events);
            initial_scroll_commands.extend(result.scroll_commands);
            let popups = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_popup_events)
                .unwrap_or_default();
            self.queue_popup_requests(popups)?;
            let messages = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_post_message_events)
                .unwrap_or_default();
            self.queue_post_message_requests_from_origin(messages, &prepared.resource.origin)?;
            let window_closes = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_close_events)
                .unwrap_or_default();
            self.queue_window_close_requests(window_closes)?;
            let window_navigations = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_navigation_events)
                .unwrap_or_default();
            self.queue_window_navigation_requests(window_navigations)?;
            if let Some(javascript) = javascript.as_ref() {
                self.config.window_name = javascript.window_name();
            }
            result.navigation
        } else {
            None
        };
        let fragment_scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task_async(NativeTask::CommitNavigation, "navigation", worker)
            .await?;
        let revision = prepared.document.revision();
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.workers.clear();
        self.pending_message_port_messages.clear();
        self.javascript = javascript;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
        self.embedding_base_url = None;
        self.scroll_offset = fragment_scroll_offset;
        self.sync_javascript_scroll_offset();
        let _ = self.apply_scroll_commands(&self.document.clone(), &initial_scroll_commands)?;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        match history_commit {
            HistoryCommit::Push => {
                self.history
                    .push(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Replace => {
                self.history
                    .replace_current(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Activate(_) => {
                return Err(NativeEngineError::Scheduler {
                    reason: "full navigation cannot activate a history entry".into(),
                });
            }
        }
        self.sync_javascript_history();
        self.sync_content_history_async().await?;
        self.flush_pending_lifecycle_effects();
        self.record_initial_events(initial_events)?;
        if execute_page_scripts {
            if let Some(javascript) = self.javascript.as_mut() {
                javascript.reset_timer_clock();
            }
            let mut page_navigation = match page_navigation {
                Some(page_navigation) => Some(self.page_navigation_request(page_navigation)?),
                None => self.dispatch_local_page_show()?,
            };
            if let Some(import_navigation) =
                self.apply_local_script_commands(&initial_module_fetches, true)?
            {
                if page_navigation.is_some() {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "one navigation commit cannot activate multiple page navigations"
                            .into(),
                    });
                }
                page_navigation = Some(import_navigation);
            }
            if let Some(javascript) = self.javascript.as_mut() {
                javascript.reset_timer_clock();
            }
            self.persist_local_web_storage()?;
            return Ok(page_navigation);
        }
        Ok(None)
    }
}
