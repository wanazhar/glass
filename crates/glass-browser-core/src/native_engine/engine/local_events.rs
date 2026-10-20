use super::*;

impl NativeEngine {
    pub(super) fn dispatch_local_events(
        &mut self,
        events: &[(NativeNodeId, NativeEventKind)],
    ) -> Result<(), NativeEngineError> {
        let document = self.document.clone();
        let Some(evaluation) = self.evaluate_local_events(&document, events)? else {
            return Ok(());
        };
        self.apply_local_script_commands(&evaluation.commands, false)
            .map(|_| ())
    }

    pub(super) fn dispatch_local_navigation_lifecycle(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        self.outgoing_lifecycle_dispatch_depth += 1;
        let result = self.dispatch_local_navigation_lifecycle_inner();
        self.outgoing_lifecycle_dispatch_depth -= 1;
        result
    }

    pub(super) fn dispatch_local_navigation_lifecycle_inner(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let lifecycle_events = [
            (window, NativeEventKind::PageHide),
            (window, NativeEventKind::Unload),
        ];
        let document = self.document.clone();
        let Some(evaluation) = self.evaluate_local_events(&document, &lifecycle_events)? else {
            return Ok(None);
        };
        let history_commands = extract_local_history_commands(&evaluation.commands);
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        effects.extend(lifecycle_events);
        if evaluation.commands.is_empty() {
            self.pending_lifecycle_effects.extend(effects);
            return Ok(navigation);
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share a lifecycle event with navigation"
                        .into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    pub(super) fn dispatch_local_before_unload(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        self.outgoing_lifecycle_dispatch_depth += 1;
        let result = self.dispatch_local_before_unload_inner();
        self.outgoing_lifecycle_dispatch_depth -= 1;
        result
    }

    pub(super) fn dispatch_local_before_unload_inner(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let document = self.document.clone();
        let Some(evaluation) =
            self.evaluate_local_events(&document, &[(window, NativeEventKind::BeforeUnload)])?
        else {
            return Ok((true, None));
        };
        let allowed = evaluation
            .value
            .as_array()
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native beforeunload".into(),
                reason: "beforeunload event result was invalid".into(),
            })?;
        let history_commands = extract_local_history_commands(&evaluation.commands);
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        effects.push((window, NativeEventKind::BeforeUnload));
        if evaluation.commands.is_empty() {
            self.record_effects(effects);
            return Ok((allowed, navigation));
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share beforeunload navigation".into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok((allowed, navigation))
    }

    pub(super) fn dispatch_local_page_show(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if self.javascript.is_none() {
            return Ok(None);
        }
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let event = (window, NativeEventKind::PageShow);
        let document = self.document.clone();
        let navigation = self
            .evaluate_local_events(&document, &[event])?
            .map(|evaluation| self.apply_local_script_commands(&evaluation.commands, true))
            .transpose()?
            .flatten();
        self.record_effects(vec![event]);
        Ok(navigation)
    }

    pub(super) fn dispatch_local_pop_state(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if self.javascript.is_none() {
            return Ok(None);
        }
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let event = (window, NativeEventKind::PopState);
        let document = self.document.clone();
        let Some(evaluation) = self.evaluate_local_events(&document, &[event])? else {
            return Ok(None);
        };
        let history_commands = extract_local_history_commands(&evaluation.commands);
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        effects.push(event);
        if evaluation.commands.is_empty() {
            self.record_effects(effects);
            return Ok(navigation);
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share popstate navigation".into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    pub(super) fn dispatch_local_hash_change(
        &mut self,
        old_url: &str,
        new_url: &str,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if self.javascript.is_none() {
            return Ok(None);
        }
        let javascript = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime is present");
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        let page_events = NativePageEventBatch {
            hash_change_events: vec![NativeHashChangeEvent {
                old_url: old_url.to_owned(),
                new_url: new_url.to_owned(),
            }],
            ..NativePageEventBatch::default()
        };
        let evaluation = {
            let javascript = self
                .javascript
                .as_ref()
                .expect("local JavaScript runtime is present");
            javascript.set_sync_xhr_loader(&self.loader);
            let result = javascript.evaluate_with_page_events(
                "undefined;",
                &self.document,
                new_url,
                &self.origin,
                self.config.viewport,
                &page_events,
            );
            if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            result?
        };
        let history_commands = extract_local_history_commands(&evaluation.commands);
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        let event = (
            NativeNodeId::from_parts(self.document.generation(), u32::MAX),
            NativeEventKind::HashChange,
        );
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        if evaluation.commands.is_empty() {
            self.record_effects(vec![event]);
            return Ok(navigation);
        }
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        effects.push(event);
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share hashchange navigation".into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    pub(super) fn evaluate_local_events(
        &mut self,
        document: &NativeDocument,
        events: &[(NativeNodeId, NativeEventKind)],
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_metadata = events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        let Some(event_batch) = host_event_batch(&event_metadata)? else {
            return Ok(None);
        };
        self.evaluate_local_host_events(document, &event_batch)
    }

    pub(super) fn evaluate_local_transition_event(
        &mut self,
        document: &NativeDocument,
        events: &[(NativeNodeId, NativeEventKind)],
        index: usize,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_metadata = events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        let Some(event_batch) = host_event_batch_at(&event_metadata, index)? else {
            return Ok(None);
        };
        self.evaluate_local_host_events(document, &event_batch)
    }

    pub(super) fn evaluate_local_host_events(
        &mut self,
        document: &NativeDocument,
        event_batch: &[NativeHostEvent],
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    pub(super) fn evaluate_local_click_event(
        &mut self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        modifiers: u8,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_batch = host_click_event_batch_with_modifiers(node_id.index(), modifiers)?;
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    pub(super) fn evaluate_local_submit_event(
        &mut self,
        document: &NativeDocument,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let Some(event_batch) =
            host_submit_event_batch(form_id.index(), submitter.map(NativeNodeId::index))?
        else {
            return Ok(None);
        };
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    pub(super) fn evaluate_local_key_event_with_modifiers(
        &mut self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        kind: NativeEventKind,
        key: &str,
        modifiers: i64,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let Some(event_batch) =
            host_key_event_batch_with_modifiers(node_id.index(), kind, key, modifiers)?
        else {
            return Ok(None);
        };
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }
}
