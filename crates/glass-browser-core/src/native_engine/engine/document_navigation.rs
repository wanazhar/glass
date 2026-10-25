use super::*;

impl NativeEngine {
    pub(super) fn apply_content_process_mutation(
        &mut self,
        mutation: NativeContentMutation,
    ) -> Result<(), NativeEngineError> {
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_at(next_revision, mutation)?;
        Ok(())
    }

    pub(super) async fn apply_content_process_mutation_async_at(
        &mut self,
        next_revision: u64,
        mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.apply_content_process_mutation_async_at_with_lifecycle(next_revision, mutation, false)
            .await
    }

    pub(super) async fn apply_content_process_lifecycle_mutation_async_at(
        &mut self,
        next_revision: u64,
        mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.apply_content_process_mutation_async_at_with_lifecycle(next_revision, mutation, true)
            .await
    }

    pub(super) async fn apply_content_process_mutation_async_at_with_lifecycle(
        &mut self,
        next_revision: u64,
        mut mutation: NativeContentMutation,
        outgoing_lifecycle: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let csp_violations = std::mem::take(&mut mutation.csp_violations);
        let history = std::mem::take(&mut mutation.history);
        if history.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryGo { delta } if *delta != 0
            )
        }) && mutation.navigation.is_some()
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share a content event with navigation".into(),
            });
        }
        let outcome = self.apply_content_process_mutation_at(next_revision, mutation)?;
        self.dispatch_content_csp_violations_async(csp_violations)
            .await?;
        let history_traversal = self.apply_content_history_commands(&history)?;
        if outgoing_lifecycle
            && history_traversal.is_some_and(|delta| self.history_delta_crosses_document(delta))
        {
            return Err(NativeEngineError::invalid(
                "native history traversal",
                "cross-document traversal cannot re-enter an outgoing lifecycle callback",
            ));
        }
        if !history.is_empty() {
            self.sync_content_history_async().await?;
        }
        if let Some(delta) = history_traversal
            && delta != 0
        {
            Box::pin(self.traverse_history_delta_async(delta)).await?;
        }
        Ok(outcome)
    }

    pub(super) fn history_delta_crosses_document(&self, delta: i32) -> bool {
        if delta == 0 || delta.unsigned_abs() as usize > MAX_NATIVE_HISTORY_DELTA as usize {
            return false;
        }
        let Some(current_index) = self.history.current_index() else {
            return false;
        };
        let Some(current_document_id) = self
            .history
            .entry(current_index)
            .map(|entry| entry.document_id)
        else {
            return false;
        };
        let mut index = current_index;
        for _ in 0..(delta.unsigned_abs() as usize).min(self.history.len()) {
            index = if delta < 0 {
                let Some(previous) = index.checked_sub(1) else {
                    break;
                };
                previous
            } else {
                let Some(next) = index.checked_add(1) else {
                    break;
                };
                next
            };
            let Some(entry) = self.history.entry(index) else {
                break;
            };
            if entry.document_id != current_document_id {
                return true;
            }
        }
        false
    }

    pub(super) fn apply_content_history_commands(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<Option<i32>, NativeEngineError> {
        let prepared = self.prepare_local_history_commands(commands)?;
        let revision = self.revision;
        self.apply_prepared_history_commands(prepared, revision)
    }

    pub(super) fn apply_prepared_history_commands(
        &mut self,
        prepared: Vec<LocalHistoryCommand>,
        revision: u64,
    ) -> Result<Option<i32>, NativeEngineError> {
        let mut traversal = None;
        for command in prepared {
            match command {
                LocalHistoryCommand::PushState { url, state } => {
                    self.url = url.clone();
                    self.history.push_with_state(
                        url,
                        revision,
                        self.scroll_offset,
                        &self.nested_scroll_offsets,
                        state,
                        true,
                    );
                }
                LocalHistoryCommand::ReplaceState { url, state } => {
                    self.url = url.clone();
                    self.history.replace_current_with_state(
                        url,
                        revision,
                        self.scroll_offset,
                        &self.nested_scroll_offsets,
                        state,
                        true,
                    );
                }
                LocalHistoryCommand::Go(delta) => traversal = Some(delta),
            }
        }
        self.sync_javascript_history();
        Ok(traversal)
    }

    pub(super) fn apply_local_history_commands_at(
        &mut self,
        commands: &[NativeScriptCommand],
        revision: u64,
    ) -> Result<Option<i32>, NativeEngineError> {
        let prepared = self.prepare_local_history_commands(commands)?;
        self.apply_prepared_history_commands(prepared, revision)
    }

    pub(super) async fn sync_content_scroll_offsets_async(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        if let Some(process) = self.content_process.as_mut() {
            process.sync_scroll_offset(self.scroll_offset).await?;
            process
                .sync_nested_scroll_offsets(&self.nested_scroll_offsets)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn sync_content_history_async(&mut self) -> Result<(), NativeEngineError> {
        let state = self
            .history
            .current()
            .map(|entry| entry.state.clone())
            .unwrap_or(serde_json::Value::Null);
        if let Some(process) = self.content_process.as_mut() {
            process
                .sync_history(&self.url, &state, self.history.len())
                .await?;
        }
        Ok(())
    }

    pub(super) fn apply_content_process_mutation_at(
        &mut self,
        next_revision: u64,
        mut mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let navigate_to_sources = mutation.navigate_to_sources.take();
        self.config.window_name = mutation.window_name.clone();
        self.queue_popup_requests(std::mem::take(&mut mutation.popups))?;
        self.queue_post_message_requests(std::mem::take(&mut mutation.post_messages))?;
        self.queue_window_close_requests(std::mem::take(&mut mutation.window_closes))?;
        self.queue_window_navigation_requests(std::mem::take(&mut mutation.window_navigations))?;
        self.publish_content_state(&mutation.storage_events, &mutation.indexed_db_changes)?;
        let dialogs = mutation.dialogs.clone();
        let scroll_commands = std::mem::take(&mut mutation.scroll_commands);
        let generation = self.document.generation();
        let native_document_state_replayable = mutation.document.native_document_state_replayable;
        let mut document = if native_document_state_replayable {
            self.document.clone()
        } else {
            match NativeDocument::from_content_wire(
                mutation.document,
                &self.config.limits,
                generation,
            ) {
                Ok(document) => document,
                Err(error) => {
                    self.content_process.take();
                    return Err(error);
                }
            }
        };
        document.set_css_target_from_url(&self.url)?;
        let mut events = match mutation
            .events
            .into_iter()
            .map(|event| {
                let node_id = NativeNodeId::from_parts(generation, event.node_index);
                if event.node_index == u32::MAX || document.node(node_id).is_some() {
                    Ok((node_id, event.kind))
                } else {
                    Err(NativeEngineError::DetachedTarget)
                }
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(events) => events,
            Err(error) => {
                self.content_process.take();
                return Err(error);
            }
        };
        self.document_navigate_to_sources = navigate_to_sources;
        let document_changed = {
            let mut normalized = document.clone();
            normalized.set_revision(self.document.revision());
            normalized != self.document
        };
        let lifecycle_only = events.iter().all(|(_, kind)| {
            matches!(
                kind,
                NativeEventKind::ReadyStateChange
                    | NativeEventKind::DomContentLoaded
                    | NativeEventKind::Load
                    | NativeEventKind::PageHide
                    | NativeEventKind::Unload
                    | NativeEventKind::PageShow
                    | NativeEventKind::BeforeUnload
                    | NativeEventKind::HashChange
                    | NativeEventKind::PopState
            )
        });
        if !document_changed && lifecycle_only && scroll_commands.is_empty() {
            let defer = events.iter().all(|(_, kind)| {
                matches!(kind, NativeEventKind::PageHide | NativeEventKind::Unload)
            });
            if defer {
                self.pending_lifecycle_effects.extend(events);
            } else {
                self.record_effects(events);
            }
            let dialog_url = self.url.clone();
            self.install_dialogs(dialogs, &dialog_url)?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        let scroll_events = self.apply_scroll_commands(&self.document.clone(), &scroll_commands)?;
        events.extend(scroll_events);
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.record_effects(events);
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    pub(super) fn activate_link(
        &mut self,
        id: super::super::dom::NativeNodeId,
        href: &str,
        click_already_applied: bool,
        force_new_target: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let target_url = self.resolve_link_href(href)?;
        let revision = if click_already_applied {
            self.revision
        } else {
            self.next_revision()?
        };
        if !self.allows_top_level_navigation(&target_url)? {
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.revision = revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        if let Some(download_attribute) =
            self.document.link_download_attribute(id).map(str::to_owned)
        {
            if self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS {
                return Err(NativeEngineError::limit(
                    "native pending downloads",
                    MAX_NATIVE_PENDING_DOWNLOADS,
                    self.pending_downloads.len().saturating_add(1),
                ));
            }
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.revision = revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            self.queue_download(target_url, &download_attribute)?;
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        if force_new_target || self.document.link_opens_new_target(id) {
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.revision = revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            self.queue_popup(target_url)?;
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        if !self.allows_frame_navigation(&target_url)? {
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            let revision = if click_already_applied {
                self.revision
            } else {
                self.document.set_revision(revision);
                self.revision = revision;
                self.history
                    .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
                self.record_effects(events);
                return Ok(NativeActionResult {
                    revision,
                    accepted: true,
                });
            };
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        let resource = self.loader.load(&target_url)?;
        if self.is_same_document_navigation(&resource.url) {
            let scroll_offset = self.fragment_scroll_offset(&resource.url)?;
            self.run_commit_task(
                NativeTask::CommitSameDocumentNavigation,
                "link same-document navigation",
            )?;
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.url = resource.url.clone();
            self.scroll_offset = scroll_offset;
            self.sync_javascript_scroll_offset();
            self.revision = revision;
            self.history.push_same_document(
                resource.url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
            );
            self.sync_javascript_history();
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }

        let prepared = self.prepare_navigation_resource_at_revision(resource, revision)?;
        let scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "link navigation")?;
        if !click_already_applied {
            let _events = self.document.apply_click(id)?;
        }
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.javascript = None;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
        self.scroll_offset = scroll_offset;
        self.sync_javascript_scroll_offset();
        self.revision = revision;
        self.history.push(self.url.clone(), revision, scroll_offset);
        Ok(NativeActionResult {
            revision,
            accepted: true,
        })
    }

    pub(super) fn resolve_link_href(&self, href: &str) -> Result<String, NativeEngineError> {
        validate_url_text("link href", href)?;
        if let Some(fragment) = href.strip_prefix('#') {
            let target = format!("{}#{fragment}", without_fragment(&self.url));
            validate_url_text("link target URL", &target)?;
            return Ok(target);
        }
        if url::Url::parse(href).is_ok() {
            return Ok(href.to_owned());
        }
        if let Ok(base) = url::Url::parse(without_fragment(&self.url))
            && (is_network_url(base.as_str()) || is_file_url(base.as_str()))
        {
            let resolved = base
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "relative network link reference is malformed".into(),
                })?;
            let resolved = resolved.to_string();
            validate_url_text("link target URL", &resolved)?;
            return Ok(resolved);
        }
        resolve_fixture_relative_url(&self.url, href)
    }

    pub(super) fn resolve_history_href_from(
        &self,
        base_url: &str,
        href: &str,
    ) -> Result<String, NativeEngineError> {
        validate_url_text("history URL", href)?;
        let target = self.resolve_page_navigation_href(base_url, href)?;
        let base = url::Url::parse(base_url).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "history base URL is malformed".into(),
        })?;
        let target_url =
            url::Url::parse(&target).map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "history target URL is malformed".into(),
            })?;
        let target_origin = NativeOrigin::from_url(&target_url)?;
        let same_origin = if self.origin == NativeOrigin::Opaque {
            target_origin == NativeOrigin::Opaque
                && base.scheme() == target_url.scheme()
                && base.host_str() == target_url.host_str()
                && base.port_or_known_default() == target_url.port_or_known_default()
        } else {
            target_origin == self.origin
        };
        if !same_origin {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "history URL must be same-origin".into(),
            });
        }
        Ok(target)
    }

    pub(super) fn resolve_page_navigation_href(
        &self,
        base_url: &str,
        href: &str,
    ) -> Result<String, NativeEngineError> {
        validate_url_text("page navigation href", href)?;
        if let Some(fragment) = href.strip_prefix('#') {
            let target = format!("{}#{fragment}", without_fragment(base_url));
            validate_url_text("page navigation target URL", &target)?;
            return Ok(target);
        }
        if url::Url::parse(href).is_ok() {
            return Ok(href.to_owned());
        }
        if let Ok(base) = url::Url::parse(without_fragment(base_url))
            && (is_network_url(base.as_str()) || is_file_url(base.as_str()))
        {
            let resolved = base
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "relative page navigation reference is malformed".into(),
                })?;
            let resolved = resolved.to_string();
            validate_url_text("page navigation target URL", &resolved)?;
            return Ok(resolved);
        }
        resolve_fixture_relative_url(base_url, href)
    }

    pub(super) fn content_navigation_request(
        &self,
        navigation: NativeContentNavigation,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        if !navigation.location {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "content lifecycle navigation must target the current browsing context"
                    .into(),
            });
        }
        let mut request = NativeNavigationRequest::get(
            self.resolve_page_navigation_href(&self.url, &navigation.href)?,
        );
        request.replace_history = navigation.replace_history;
        request.object_url = navigation.object_url;
        Ok(request)
    }

    pub(super) fn page_navigation_request(
        &self,
        navigation: NativePageNavigation,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        let mut request = NativeNavigationRequest::get(
            self.resolve_page_navigation_href(&self.url, &navigation.href)?,
        );
        request.replace_history = navigation.replace_history;
        request.object_url = navigation.object_url;
        Ok(request)
    }

    pub(super) fn navigate_page_script_sync(
        &mut self,
        mut navigation: NativeNavigationRequest,
        mut page_navigation_handoffs: usize,
    ) -> Result<(), NativeEngineError> {
        loop {
            if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                return Err(NativeEngineError::limit(
                    "page navigation handoffs",
                    MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                    page_navigation_handoffs,
                ));
            }
            if is_network_url(&navigation.url) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason:
                        "synchronous page-script navigation requires an asynchronous native owner"
                            .into(),
                });
            }
            if !self.allows_top_level_navigation(&navigation.url)? {
                return Ok(());
            }
            let history_commit = if navigation.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            let resource = self.loader.load_navigation(&navigation)?;
            if self.is_same_document_navigation(&resource.url) {
                let _ = std::mem::take(&mut self.skip_next_navigation_lifecycle);
                if let Some(next_navigation) =
                    self.commit_same_document_navigation(resource.url, history_commit)?
                {
                    navigation = next_navigation;
                    page_navigation_handoffs = page_navigation_handoffs.saturating_add(1);
                    continue;
                }
                return Ok(());
            }
            let prepared = self.prepare_navigation_resource(resource)?;
            let Some(next_navigation) = self.commit_navigation(prepared, history_commit)? else {
                return Ok(());
            };
            navigation = next_navigation;
            page_navigation_handoffs = page_navigation_handoffs.saturating_add(1);
        }
    }

    pub(super) fn navigate_lifecycle_handoff_sync(
        &mut self,
        navigation: NativeNavigationRequest,
    ) -> Result<(), NativeEngineError> {
        let previous_skip = std::mem::replace(&mut self.skip_next_navigation_lifecycle, true);
        let result = self.navigate_page_script_sync(navigation, 1);
        if self.skip_next_navigation_lifecycle {
            self.skip_next_navigation_lifecycle = previous_skip;
        }
        result
    }

    pub(super) fn synchronous_beforeunload_error() -> NativeEngineError {
        NativeEngineError::invalid(
            "native beforeunload prompt",
            "synchronous navigation cannot wait for a user decision; use an async native session with a responsive dialog controller",
        )
    }
}
