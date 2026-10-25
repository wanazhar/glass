use super::*;

impl NativeEngine {
    pub fn action(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        let (action, click_modifiers) = match action {
            NativeAction::ClickWithModifiers { target, modifiers } => {
                if modifiers > 15 {
                    return Err(NativeEngineError::invalid(
                        "native click modifiers",
                        "must be a bit mask from 0 through 15",
                    ));
                }
                (NativeAction::Click { target }, modifiers)
            }
            action => (action, 0),
        };
        self.scroll_action_targets_into_view(&action)?;
        if let NativeAction::DoubleClick { target } = action {
            self.action(NativeAction::Click {
                target: target.clone(),
            })?;
            return self.action(NativeAction::Click { target });
        }
        let (events, accepted) = match action {
            NativeAction::DoubleClick { .. } => {
                unreachable!("double-click is handled before the native action match")
            }
            NativeAction::ClickWithModifiers { .. } => {
                unreachable!("modifier clicks are normalized before the native action match")
            }
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                self.require_layout_actionable(id)?;
                self.document_has_sticky_activation = true;
                if let Some(href) = self.document.link_href(id).map(str::to_owned)
                    && !href.is_empty()
                {
                    if self.javascript.is_some() {
                        return self.action_local_click_with_event_preflight(id, click_modifiers);
                    }
                    return self.activate_link(
                        id,
                        &href,
                        false,
                        modifier_click_opens_new_target(click_modifiers),
                    );
                }
                if let Some(form_id) = self.document.submit_control_form(id)
                    && self.javascript.is_none()
                {
                    return self.action_local_submit_click_without_script(id, form_id);
                }
                if self.javascript.is_some() {
                    return self.action_local_click_with_event_preflight(id, click_modifiers);
                }
                (self.document.apply_click(id)?, true)
            }
            NativeAction::Hover { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_hover(id),
                        "native hover event effects",
                    );
                }
                (self.document.apply_hover(id)?, true)
            }
            NativeAction::Drag {
                source,
                destination,
            } => {
                let source_id = self.document.resolve_target(&source)?;
                let destination_id = self.document.resolve_target(&destination)?;
                self.require_layout_actionable(source_id)?;
                self.require_layout_actionable(destination_id)?;
                self.document_has_sticky_activation = true;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_drag(source_id, destination_id),
                        "native drag event effects",
                    );
                }
                (self.document.apply_drag(source_id, destination_id)?, true)
            }
            NativeAction::Upload { target, files } => {
                let id = self.document.resolve_target(&target)?;
                self.document_has_sticky_activation = true;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_upload(id, &files),
                        "native upload event effects",
                    );
                }
                (self.document.apply_upload(id, &files)?, true)
            }
            NativeAction::Type { target, text } => {
                let id = if target == "focused" {
                    self.document.focused_text_control()?
                } else {
                    self.document.resolve_target(&target)?
                };
                self.require_layout_actionable(id)?;
                self.document_has_sticky_activation = true;
                if self.javascript.is_some() {
                    return self.action_local_type_with_event_transaction(id, &text);
                }
                (self.document.apply_type(id, &text)?, true)
            }
            NativeAction::Clear { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_clear(id),
                        "native clear event effects",
                    );
                }
                (self.document.apply_clear(id)?, true)
            }
            NativeAction::Check { target } => {
                return self.action_checked_with_click(target, true);
            }
            NativeAction::Uncheck { target } => {
                return self.action_checked_with_click(target, false);
            }
            NativeAction::Select { target, value } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                self.document_has_sticky_activation = true;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_select(id, &value),
                        "native select event effects",
                    );
                }
                (self.document.apply_select(id, &value)?, true)
            }
            NativeAction::KeyDown { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                self.document_has_sticky_activation = true;
                let id = self.document.focused_node();
                return self.action_local_key_event(id, &key, NativeEventKind::KeyDown, 0);
            }
            NativeAction::KeyUp { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                let id = self.document.focused_node();
                return self.action_local_key_event(id, &key, NativeEventKind::KeyUp, 0);
            }
            NativeAction::Shortcut { shortcut } => {
                let (modifiers, key) = parse_native_shortcut(&shortcut)?;
                self.document_has_sticky_activation = true;
                let id = self.document.focused_node();
                return self.action_local_key_sequence(id, &key, modifiers, true);
            }
            NativeAction::KeyPress { key } => {
                validate_native_edit_key(&key)?;
                let id = self.document.focused_text_control()?;
                self.document_has_sticky_activation = true;
                return self.action_local_key_sequence(id, &key, 0, true);
            }
            NativeAction::Scroll { delta_x, delta_y } => {
                let moved = self.apply_scroll(delta_x, delta_y)?;
                let events = moved
                    .then(|| {
                        (
                            NativeNodeId::from_parts(self.document.generation(), u32::MAX),
                            NativeEventKind::Scroll,
                        )
                    })
                    .into_iter()
                    .collect();
                (events, moved)
            }
            NativeAction::ScrollTarget {
                target,
                delta_x,
                delta_y,
            } => {
                let id = self.document.resolve_target(&target)?;
                let moved = self.apply_scroll_target(id, delta_x, delta_y)?;
                let events = moved
                    .then_some((id, NativeEventKind::Scroll))
                    .into_iter()
                    .collect();
                (events, moved)
            }
        };
        if !accepted {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: false,
            });
        }
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.record_effects(events.clone());
        self.dispatch_local_events(&events)?;
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    /// Apply an action through the child-owned document when the current
    /// navigation is process-backed. Navigation actions remain parent-owned so
    /// a link cannot mutate the child document without also committing a new
    /// resource and history entry.
    pub async fn action_async(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        let (action, click_modifiers) = match action {
            NativeAction::ClickWithModifiers { target, modifiers } => {
                if modifiers > 15 {
                    return Err(NativeEngineError::invalid(
                        "native click modifiers",
                        "must be a bit mask from 0 through 15",
                    ));
                }
                (NativeAction::Click { target }, modifiers)
            }
            action => (action, 0),
        };
        self.scroll_action_targets_into_view(&action)?;
        if let NativeAction::DoubleClick { target } = action {
            Box::pin(self.action_async(NativeAction::Click {
                target: target.clone(),
            }))
            .await?;
            return Box::pin(self.action_async(NativeAction::Click { target })).await;
        }
        self.sync_external_storage_events()?;
        if self.content_process.is_some() {
            self.deliver_pending_external_storage_events().await?;
        }
        let Some(process) = self.content_process.as_mut() else {
            return if click_modifiers == 0 {
                self.action(action)
            } else if let NativeAction::Click { target } = action {
                self.action(NativeAction::ClickWithModifiers {
                    target,
                    modifiers: click_modifiers,
                })
            } else {
                unreachable!("only click actions carry pointer modifiers")
            };
        };
        if !process.refresh_health() {
            return Err(NativeEngineError::worker_failure(
                "content process mutation",
                process
                    .failure_kind()
                    .unwrap_or(NativeWorkerFailureKind::Exited),
                "content process is unavailable after a failed mutation; navigate to recover it",
            ));
        }
        self.content_process
            .as_mut()
            .expect("content process presence was checked")
            .sync_scroll_offset(self.scroll_offset)
            .await?;
        self.content_process
            .as_mut()
            .expect("content process presence was checked")
            .sync_nested_scroll_offsets(&self.nested_scroll_offsets)
            .await?;
        match action {
            NativeAction::DoubleClick { .. } => {
                unreachable!("double-click is handled before the native async action match")
            }
            NativeAction::ClickWithModifiers { .. } => {
                unreachable!("modifier clicks are normalized before the native async action match")
            }
            NativeAction::Check { target } => {
                self.action_checked_async_with_click(target, true).await
            }
            NativeAction::Uncheck { target } => {
                self.action_checked_async_with_click(target, false).await
            }
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                self.require_layout_actionable(id)?;
                let initial_link_href = self
                    .document
                    .link_href(id)
                    .filter(|href| !href.is_empty())
                    .map(str::to_owned);
                let initial_download_attribute =
                    self.document.link_download_attribute(id).map(str::to_owned);
                if initial_download_attribute.is_some()
                    && self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS
                {
                    return Err(NativeEngineError::limit(
                        "native pending downloads",
                        MAX_NATIVE_PENDING_DOWNLOADS,
                        self.pending_downloads.len().saturating_add(1),
                    ));
                }
                let mut preview = self.document.clone();
                preview.apply_click(id)?;
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process click preflight".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_click_with_event_preflight(
                            id.index(),
                            click_modifiers,
                            &mut self.loader,
                        )
                        .await?
                };
                let navigation = mutation.navigation.clone();
                let click_allowed = mutation.allowed;
                if click_allowed
                    && initial_link_href.is_some()
                    && mutation.history.iter().any(|command| {
                        matches!(
                            command,
                            NativeScriptCommand::HistoryGo { delta } if *delta != 0
                        )
                    })
                {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "history traversal cannot share a content click with navigation"
                            .into(),
                    });
                }
                let next_revision = self.next_revision()?;
                let outcome = self
                    .apply_content_process_mutation_async_at(next_revision, mutation)
                    .await?;
                if let Some(navigation) = navigation {
                    self.navigate_script_navigation_async(navigation, 0).await?;
                    return Ok(NativeActionResult {
                        revision: self.revision,
                        accepted: outcome.accepted,
                    });
                }
                if click_allowed
                    && let Some(href) = self
                        .document
                        .link_href(id)
                        .filter(|href| !href.is_empty())
                        .map(str::to_owned)
                {
                    let download_attribute =
                        self.document.link_download_attribute(id).map(str::to_owned);
                    if download_attribute.is_some()
                        && self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS
                    {
                        return Err(NativeEngineError::limit(
                            "native pending downloads",
                            MAX_NATIVE_PENDING_DOWNLOADS,
                            self.pending_downloads.len().saturating_add(1),
                        ));
                    }
                    let target_url = self.resolve_link_href(&href)?;
                    let opens_new_target = self.document.link_opens_new_target(id)
                        || modifier_click_opens_new_target(click_modifiers);
                    let special_navigation = download_attribute.is_some() || opens_new_target;
                    if special_navigation
                        && !self
                            .allows_top_level_navigation_async(&target_url, true)
                            .await?
                    {
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    if let Some(download_attribute) = download_attribute {
                        self.queue_download(target_url, &download_attribute)?;
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    if opens_new_target {
                        self.queue_popup(target_url)?;
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    self.navigate_request_async(NativeNavigationRequest::get(target_url), 0)
                        .await?;
                    return Ok(NativeActionResult {
                        revision: self.revision,
                        accepted: outcome.accepted,
                    });
                }
                Ok(outcome)
            }
            NativeAction::Hover { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_hover(id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process hover event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(
                            serde_json::json!({
                                "kind": "hover",
                                "node_index": id.index(),
                            }),
                            &mut self.loader,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Drag {
                source,
                destination,
            } => {
                let source_id = self.document.resolve_target(&source)?;
                let destination_id = self.document.resolve_target(&destination)?;
                self.require_layout_actionable(source_id)?;
                self.require_layout_actionable(destination_id)?;
                let preview = self.document.clone();
                preview.apply_drag(source_id, destination_id)?;
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process drag event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(
                            serde_json::json!({
                                "kind": "drag",
                                "node_index": source_id.index(),
                                "destination_node_index": destination_id.index(),
                            }),
                            &mut self.loader,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Upload { target, files } => {
                let id = self.document.resolve_target(&target)?;
                let mut preview = self.document.clone();
                preview.apply_upload(id, &files)?;
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process upload event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(
                            serde_json::json!({
                                "kind": "upload",
                                "node_index": id.index(),
                                "files": files,
                            }),
                            &mut self.loader,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Type { target, text } => {
                let id = if target == "focused" {
                    self.document.focused_text_control()?
                } else {
                    self.document.resolve_target(&target)?
                };
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_type(id, &text)?;
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process type event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_type_with_event_bridge(id.index(), text, &mut self.loader)
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Clear { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_clear(id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process clear event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(
                            serde_json::json!({
                                "kind": "clear",
                                "node_index": id.index(),
                            }),
                            &mut self.loader,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Select { target, value } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_select(id, &value)?;
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process select event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(
                            serde_json::json!({
                                "kind": "select",
                                "node_index": id.index(),
                                "value": value,
                            }),
                            &mut self.loader,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::KeyDown { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                self.document_has_sticky_activation = true;
                let focused_node = self.document.focused_node();
                let keyboard_link_target = (key == "Enter"
                    && self
                        .document
                        .has_native_keyboard_link_activation(focused_node))
                .then_some(focused_node);
                let node_index = self.document.keyboard_event_target(focused_node).index();
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process keydown event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_event_with_event_bridge(
                            node_index,
                            key,
                            NativeEventKind::KeyDown,
                            0,
                            &mut self.loader,
                        )
                        .await?
                };
                self.apply_keyboard_mutation_async(mutation, keyboard_link_target)
                    .await
            }
            NativeAction::KeyUp { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                let focused_node = self.document.focused_node();
                let node_index = self.document.keyboard_event_target(focused_node).index();
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process keyup event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_event_with_event_bridge(
                            node_index,
                            key,
                            NativeEventKind::KeyUp,
                            0,
                            &mut self.loader,
                        )
                        .await?
                };
                self.apply_keyboard_mutation_async(mutation, None).await
            }
            NativeAction::Shortcut { shortcut } => {
                let (modifiers, key) = parse_native_shortcut(&shortcut)?;
                self.document_has_sticky_activation = true;
                let focused_node = self.document.focused_node();
                let keyboard_link_target = (key == "Enter"
                    && self
                        .document
                        .has_native_keyboard_link_activation(focused_node))
                .then_some(focused_node);
                let node_index = self.document.keyboard_event_target(focused_node).index();
                let default_allowed = should_apply_native_key_default(&key, modifiers);
                let apply_default = default_allowed
                    && (key == "Tab"
                        || self.document.can_apply_radio_group_arrow_navigation(
                            focused_node,
                            &key,
                            modifiers,
                        )
                        || self
                            .document
                            .focused_text_control()
                            .is_ok_and(|id| id.index() == node_index));
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process shortcut event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_shortcut_with_event_bridge(
                            node_index,
                            key,
                            modifiers,
                            apply_default,
                            &mut self.loader,
                        )
                        .await?
                };
                self.apply_keyboard_mutation_async(mutation, keyboard_link_target)
                    .await
            }
            NativeAction::KeyPress { key } => {
                validate_native_edit_key(&key)?;
                let id = self.document.focused_text_control()?;
                let mut preview = self.document.clone();
                preview.apply_key_press(id, &key)?;
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process key event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_with_event_bridge(id.index(), key, &mut self.loader)
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Scroll { .. } | NativeAction::ScrollTarget { .. } => self.action(action),
        }
    }

    /// Apply the browser action contract's bounded scroll-into-view step for
    /// semantic element targets. Coordinate clicks are deliberately excluded:
    /// their viewport coordinates are explicit and must never be adjusted.
    /// The scroll is folded into the following action transaction, so the
    /// action still commits one revision; read-only preflight never calls this
    /// helper.
    fn scroll_action_targets_into_view(
        &mut self,
        action: &NativeAction,
    ) -> Result<(), NativeEngineError> {
        let mut targets = Vec::new();
        match action {
            NativeAction::Click { target }
            | NativeAction::ClickWithModifiers { target, .. }
            | NativeAction::DoubleClick { target }
            | NativeAction::Hover { target }
            | NativeAction::Type { target, .. }
            | NativeAction::Clear { target }
            | NativeAction::Check { target }
            | NativeAction::Uncheck { target }
            | NativeAction::Select { target, .. } => targets.push(target.as_str()),
            NativeAction::Drag {
                source,
                destination,
            } => {
                targets.push(source.as_str());
                targets.push(destination.as_str());
            }
            NativeAction::Upload { .. }
            | NativeAction::KeyDown { .. }
            | NativeAction::KeyUp { .. }
            | NativeAction::Shortcut { .. }
            | NativeAction::KeyPress { .. }
            | NativeAction::Scroll { .. }
            | NativeAction::ScrollTarget { .. } => {}
        }
        for target in targets {
            if parse_point_target(target)?.is_some() {
                continue;
            }
            let id = self.document.resolve_target(target)?;
            self.scroll_node_into_view(id)?;
        }
        Ok(())
    }

    fn scroll_node_into_view(&mut self, id: NativeNodeId) -> Result<(), NativeEngineError> {
        if self.document.is_hidden_for_layout(id) {
            return Ok(());
        }
        let layout = self.layout()?;
        if layout.viewport_rect_for(id).is_some() {
            return Ok(());
        }
        let Some(target) = layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == id)
        else {
            return Ok(());
        };
        let max_scroll = layout.max_scroll_offset();
        let viewport_right = self.scroll_offset.x.saturating_add(layout.viewport.width);
        let viewport_bottom = self.scroll_offset.y.saturating_add(layout.viewport.height);
        let next = NativePoint {
            x: scroll_axis_into_view(
                target.rect.x,
                target.rect.right(),
                self.scroll_offset.x,
                viewport_right,
                max_scroll.x,
            ),
            y: scroll_axis_into_view(
                target.rect.y,
                target.rect.bottom(),
                self.scroll_offset.y,
                viewport_bottom,
                max_scroll.y,
            ),
        };
        if next == self.scroll_offset {
            return Ok(());
        }
        self.scroll_offset = next;
        self.sync_javascript_scroll_offset();
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        Ok(())
    }

    fn action_checked_with_click(
        &mut self,
        target: String,
        desired: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let id = self.document.resolve_target(&target)?;
        self.require_layout_actionable(id)?;
        if self.document.checked_control_state(id)? == desired {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        let outcome = self.action(NativeAction::Click { target })?;
        if self.document.checked_control_state(id)? != desired {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "checkable control did not reach the requested state".into(),
            });
        }
        Ok(outcome)
    }

    async fn action_checked_async_with_click(
        &mut self,
        target: String,
        desired: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let id = self.document.resolve_target(&target)?;
        self.require_layout_actionable(id)?;
        if self.document.checked_control_state(id)? == desired {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        let outcome = Box::pin(self.action_async(NativeAction::Click { target })).await?;
        if self.document.checked_control_state(id)? != desired {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "checkable control did not reach the requested state".into(),
            });
        }
        Ok(outcome)
    }

    fn action_local_click_with_event_preflight(
        &mut self,
        id: NativeNodeId,
        modifiers: u8,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = document.apply_script_focus(id)?;
        if let Some(evaluation) = self.evaluate_local_events(&document, &events)? {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &evaluation.commands,
                &mut history_commands,
            )?);
        }

        let checkable_pre_activation = document.pre_activate_checkable(id)?;

        let click_evaluation = self
            .evaluate_local_click_event(&document, id, modifiers)?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native click event preflight".into(),
                reason: "native JavaScript realm disappeared during click preflight".into(),
            })?;
        let click_allowed = click_evaluation
            .value
            .as_array()
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native click event preflight".into(),
                reason: "native click event result was invalid".into(),
            })?;
        events.extend(self.apply_local_evaluation_commands(
            &mut document,
            &click_evaluation.commands,
            &mut history_commands,
        )?);
        let mut navigation: Option<(NativeNodeId, NativeNodeId)> = None;
        let mut link_navigation = None;
        if click_allowed {
            let click_events = if checkable_pre_activation.is_some() {
                document.apply_click_after_checkable_pre_activation(id)?
            } else {
                document.apply_click(id)?
            };
            let input_change_events = click_events
                .iter()
                .copied()
                .filter(|(_, kind)| {
                    matches!(kind, NativeEventKind::Input | NativeEventKind::Change)
                })
                .collect::<Vec<_>>();
            events.extend(click_events);
            if !input_change_events.is_empty()
                && let Some(evaluation) =
                    self.evaluate_local_events(&document, &input_change_events)?
            {
                events.extend(self.apply_local_evaluation_commands(
                    &mut document,
                    &evaluation.commands,
                    &mut history_commands,
                )?);
            }
            if let Some(href) = document.link_href(id).filter(|href| !href.is_empty()) {
                link_navigation = Some(href.to_owned());
            } else if let Some(form_id) = document.submit_control_form(id) {
                let invalid = document.invalid_form_controls(form_id, Some(id))?;
                if invalid.is_empty() {
                    let submit_evaluation = self
                        .evaluate_local_submit_event(&document, form_id, Some(id))?
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "native submit event".into(),
                            reason: "native JavaScript realm disappeared during submit dispatch"
                                .into(),
                        })?;
                    let submit_allowed = submit_evaluation
                        .value
                        .as_array()
                        .and_then(|values| values.first())
                        .and_then(serde_json::Value::as_bool)
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "native submit event".into(),
                            reason: "native submit event result was invalid".into(),
                        })?;
                    events.push((form_id, NativeEventKind::Submit));
                    events.extend(self.apply_local_evaluation_commands(
                        &mut document,
                        &submit_evaluation.commands,
                        &mut history_commands,
                    )?);
                    if submit_allowed {
                        navigation = Some((form_id, id));
                    }
                } else {
                    let invalid_events = invalid
                        .iter()
                        .copied()
                        .map(|invalid_id| (invalid_id, NativeEventKind::Invalid))
                        .collect::<Vec<_>>();
                    if let Some(evaluation) =
                        self.evaluate_local_events(&document, &invalid_events)?
                    {
                        events.extend(invalid_events);
                        events.extend(self.apply_local_evaluation_commands(
                            &mut document,
                            &evaluation.commands,
                            &mut history_commands,
                        )?);
                    }
                }
            }
        } else {
            if let Some(checkable_pre_activation) = checkable_pre_activation {
                document.restore_checkable_pre_activation(checkable_pre_activation)?;
            }
            events.push((id, NativeEventKind::Click));
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native click event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        if let Some(href) = link_navigation.as_deref() {
            self.preflight_local_link_navigation(&document, id, href)?;
        }
        if history_commands.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryGo { delta } if *delta != 0
            )
        }) && (link_navigation.is_some() || navigation.is_some())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share a click with navigation".into(),
            });
        }

        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(href) = link_navigation {
            return self.activate_link(id, &href, true, modifier_click_opens_new_target(modifiers));
        }
        if let Some((form_id, submitter)) = navigation {
            let request = self.document.form_submission_request_with_submitter(
                form_id,
                &self.url,
                Some(submitter),
            )?;
            if !self.loader.allows_navigation(
                &self.url,
                &request.url,
                NativeNavigationPolicyKind::FormAction,
            )? {
                return Ok(NativeActionResult {
                    revision: self.revision,
                    accepted: true,
                });
            }
            return self.navigate_local_form_request(request);
        }
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    /// Validate the synchronous local default-navigation path before running
    /// click handlers. Event dispatch is transactional from the caller's
    /// perspective: an unsupported destination must not leave focus, event
    /// effects, or a revision behind merely because the handler ran first.
    pub(super) fn preflight_local_link_navigation(
        &self,
        document: &NativeDocument,
        id: NativeNodeId,
        href: &str,
    ) -> Result<(), NativeEngineError> {
        if document.link_download_attribute(id).is_some() || document.link_opens_new_target(id) {
            return Ok(());
        }
        let target_url = self.resolve_link_href(href)?;
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(());
        }
        if !self.allows_top_level_navigation_silent(&target_url)? {
            return Ok(());
        }
        self.loader.load(&target_url).map(|_| ())
    }

    pub(super) fn action_local_submit_click_without_script(
        &mut self,
        id: NativeNodeId,
        form_id: NativeNodeId,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let events = self.document.apply_click(id)?;
        if !self
            .document
            .invalid_form_controls(form_id, Some(id))?
            .is_empty()
        {
            let next_revision = self.next_revision()?;
            self.document.set_revision(next_revision);
            self.revision = next_revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision: next_revision,
                accepted: true,
            });
        }
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.record_effects(events);
        let request =
            self.document
                .form_submission_request_with_submitter(form_id, &self.url, Some(id))?;
        if !self.loader.allows_navigation(
            &self.url,
            &request.url,
            NativeNavigationPolicyKind::FormAction,
        )? {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        self.navigate_local_form_request(request)
    }

    pub(super) fn action_local_type_with_event_transaction(
        &mut self,
        id: NativeNodeId,
        text: &str,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = document.apply_type(id, text)?;
        let default_events = events.clone();
        for event in default_events {
            if let Some(evaluation) = self.evaluate_local_events(&document, &[event])? {
                events.extend(self.apply_local_evaluation_commands(
                    &mut document,
                    &evaluation.commands,
                    &mut history_commands,
                )?);
            }
            if events.len() > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native type event effects",
                    MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    pub(super) fn action_local_form_with_event_transaction(
        &mut self,
        apply: impl FnOnce(
            &mut NativeDocument,
        ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError>,
        effect_limit_name: &str,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = apply(&mut document)?;
        let default_events = events.clone();
        for index in 0..default_events.len() {
            if let Some(evaluation) =
                self.evaluate_local_transition_event(&document, &default_events, index)?
            {
                events.extend(self.apply_local_evaluation_commands(
                    &mut document,
                    &evaluation.commands,
                    &mut history_commands,
                )?);
            }
            if events.len() > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    effect_limit_name,
                    MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    pub(super) fn apply_local_keyboard_activation(
        &mut self,
        document: &mut NativeDocument,
        id: NativeNodeId,
        events: &mut Vec<(NativeNodeId, NativeEventKind)>,
        history_commands: &mut Vec<NativeScriptCommand>,
    ) -> Result<Option<NativeLocalKeyboardNavigation>, NativeEngineError> {
        if document.focused_node() != id
            || (!document.has_native_keyboard_button_activation(id)
                && !document.has_native_keyboard_link_activation(id)
                && !document.has_native_keyboard_checkable_activation(id))
        {
            return Ok(None);
        }

        let checkable_pre_activation = document.pre_activate_checkable(id)?;

        let click_evaluation =
            self.evaluate_local_events(document, &[(id, NativeEventKind::Click)])?;
        let click_allowed = if let Some(evaluation) = click_evaluation.as_ref() {
            evaluation
                .value
                .as_array()
                .and_then(|values| values.first())
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "native keyboard click preflight".into(),
                    reason: "native click event result was invalid".into(),
                })?
        } else {
            true
        };
        if let Some(evaluation) = click_evaluation {
            events.extend(self.apply_local_evaluation_commands(
                document,
                &evaluation.commands,
                history_commands,
            )?);
        }

        let mut navigation = None;
        if click_allowed {
            let click_events = if checkable_pre_activation.is_some() {
                document.apply_click_after_checkable_pre_activation(id)?
            } else {
                document.apply_click(id)?
            };
            let input_change_events = click_events
                .iter()
                .copied()
                .filter(|(_, kind)| {
                    matches!(kind, NativeEventKind::Input | NativeEventKind::Change)
                })
                .collect::<Vec<_>>();
            events.extend(click_events);
            if !input_change_events.is_empty()
                && let Some(evaluation) =
                    self.evaluate_local_events(document, &input_change_events)?
            {
                events.extend(self.apply_local_evaluation_commands(
                    document,
                    &evaluation.commands,
                    history_commands,
                )?);
            }
            if let Some(href) = document
                .link_href(id)
                .filter(|href| !href.is_empty())
                .map(str::to_owned)
            {
                self.preflight_local_link_navigation(document, id, &href)?;
                navigation = Some(NativeLocalKeyboardNavigation::Link { target: id, href });
            } else if let Some(form_id) = document.reset_control_form(id) {
                if let Some(javascript) = self.javascript.as_ref() {
                    javascript.set_scroll_offset(self.scroll_offset);
                    javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
                    self.sync_javascript_history();
                    javascript.set_sync_xhr_loader(&self.loader);
                    let command = serde_json::json!({
                        "kind": "activateFormReset",
                        "node_index": form_id.index(),
                    });
                    let command =
                        serde_json::to_string(&command).map_err(|_| NativeEngineError::Worker {
                            operation: "activate native keyboard reset button".into(),
                            reason: "reset form target could not be encoded".into(),
                        })?;
                    let evaluation_result = javascript.evaluate(
                        &format!("globalThis.__glassApplyNativeCommand({command})"),
                        document,
                        &self.url,
                        &self.origin,
                        self.config.viewport,
                    );
                    if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
                        self.loader.merge_fetch_task_state(updated_loader)?;
                    }
                    let evaluation = evaluation_result?;
                    events.extend(self.apply_local_evaluation_commands(
                        document,
                        &evaluation.commands,
                        history_commands,
                    )?);
                    self.drain_local_popups()?;
                    self.drain_local_dialogs()?;
                    self.persist_local_script_state()?;
                } else {
                    events.push((form_id, NativeEventKind::Reset));
                    document.reset_form_controls(form_id)?;
                }
            } else if let Some(form_id) = document.submit_control_form(id) {
                let invalid = document.invalid_form_controls(form_id, Some(id))?;
                if invalid.is_empty() {
                    let submit_evaluation =
                        self.evaluate_local_submit_event(document, form_id, Some(id))?;
                    let submit_allowed = if let Some(evaluation) = submit_evaluation.as_ref() {
                        evaluation
                            .value
                            .as_array()
                            .and_then(|values| values.first())
                            .and_then(serde_json::Value::as_bool)
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "native keyboard submit event".into(),
                                reason: "native submit event result was invalid".into(),
                            })?
                    } else {
                        true
                    };
                    events.push((form_id, NativeEventKind::Submit));
                    if let Some(evaluation) = submit_evaluation {
                        events.extend(self.apply_local_evaluation_commands(
                            document,
                            &evaluation.commands,
                            history_commands,
                        )?);
                    }
                    if submit_allowed {
                        navigation = Some(NativeLocalKeyboardNavigation::FormSubmit {
                            form_id,
                            submitter: id,
                        });
                    }
                } else {
                    let invalid_events = invalid
                        .iter()
                        .copied()
                        .map(|invalid_id| (invalid_id, NativeEventKind::Invalid))
                        .collect::<Vec<_>>();
                    events.extend(invalid_events.iter().copied());
                    if let Some(evaluation) =
                        self.evaluate_local_events(document, &invalid_events)?
                    {
                        events.extend(self.apply_local_evaluation_commands(
                            document,
                            &evaluation.commands,
                            history_commands,
                        )?);
                    }
                }
            }
        } else {
            if let Some(checkable_pre_activation) = checkable_pre_activation {
                document.restore_checkable_pre_activation(checkable_pre_activation)?;
            }
            events.push((id, NativeEventKind::Click));
        }

        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native keyboard activation effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        if navigation.is_some()
            && history_commands.iter().any(|command| {
                matches!(command, NativeScriptCommand::HistoryGo { delta } if *delta != 0)
            })
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share keyboard activation with navigation"
                    .into(),
            });
        }
        Ok(navigation)
    }

    pub(super) fn finish_local_keyboard_action(
        &mut self,
        document: NativeDocument,
        events: Vec<(NativeNodeId, NativeEventKind)>,
        history_commands: &[NativeScriptCommand],
        navigation: Option<NativeLocalKeyboardNavigation>,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let next_revision = self.next_revision()?;
        let mut document = document;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(history_commands, next_revision)?;
        self.record_effects(events);

        match navigation {
            Some(NativeLocalKeyboardNavigation::Link { target, href }) => {
                return self.activate_link(target, &href, true, false);
            }
            Some(NativeLocalKeyboardNavigation::FormSubmit { form_id, submitter }) => {
                let request = self.document.form_submission_request_with_submitter(
                    form_id,
                    &self.url,
                    Some(submitter),
                )?;
                if self.loader.allows_navigation(
                    &self.url,
                    &request.url,
                    NativeNavigationPolicyKind::FormAction,
                )? {
                    return self.navigate_local_form_request(request);
                }
            }
            None => {}
        }
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    pub(super) fn action_local_key_event(
        &mut self,
        id: NativeNodeId,
        key: &str,
        kind: NativeEventKind,
        modifiers: i64,
    ) -> Result<NativeActionResult, NativeEngineError> {
        validate_native_key(key)?;
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let event_target = document.keyboard_event_target(id);
        let mut events = vec![(event_target, kind)];
        let was_button = document.has_native_keyboard_button_activation(id);
        let was_link = document.has_native_keyboard_link_activation(id);
        let was_checkable_kind = document.keyboard_checkable_kind(id);
        let was_radio_arrow_target =
            document.can_apply_radio_group_arrow_navigation(id, key, modifiers);
        let evaluation = self.evaluate_local_key_event_with_modifiers(
            &document,
            event_target,
            kind,
            key,
            modifiers,
        )?;
        let event_allowed = evaluation
            .as_ref()
            .and_then(|evaluation| evaluation.value.as_array())
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if let Some(evaluation) = evaluation {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &evaluation.commands,
                &mut history_commands,
            )?);
        }

        if kind == NativeEventKind::KeyDown
            && event_allowed
            && was_radio_arrow_target
            && document.focused_node() == id
            && document.can_apply_radio_group_arrow_navigation(id, key, modifiers)
        {
            let default_events = document.apply_key_default(id, key, modifiers)?;
            events.extend(default_events.clone());
            for index in 0..default_events.len() {
                if let Some(evaluation) =
                    self.evaluate_local_transition_event(&document, &default_events, index)?
                {
                    events.extend(self.apply_local_evaluation_commands(
                        &mut document,
                        &evaluation.commands,
                        &mut history_commands,
                    )?);
                }
            }
        }

        let mut submit_navigation = None;
        if kind == NativeEventKind::KeyDown && key == " " {
            self.pending_space_activation = None;
            if event_allowed && document.focused_node() == id {
                if was_button && document.has_native_keyboard_button_activation(id) {
                    self.pending_space_activation = Some((id, None));
                } else if let Some(expected) = was_checkable_kind
                    && document.keyboard_checkable_kind(id) == Some(expected)
                {
                    self.pending_space_activation = Some((id, Some(expected)));
                }
            }
        } else if kind == NativeEventKind::KeyUp && key == " " {
            let pending = self.pending_space_activation.take();
            let pending_checkable = pending.is_some_and(|(target, expected)| {
                target == id
                    && expected.is_some_and(|expected| {
                        document.keyboard_checkable_kind(id) == Some(expected)
                    })
            });
            let pending_button =
                pending == Some((id, None)) && document.has_native_keyboard_button_activation(id);
            if (pending_button && event_allowed || pending_checkable)
                && document.focused_node() == id
            {
                submit_navigation = self.apply_local_keyboard_activation(
                    &mut document,
                    id,
                    &mut events,
                    &mut history_commands,
                )?;
            }
        } else if kind == NativeEventKind::KeyDown
            && key == "Enter"
            && (was_button || was_link)
            && event_allowed
            && document.focused_node() == id
            && (document.has_native_keyboard_button_activation(id)
                || (was_link && document.has_native_keyboard_link_activation(id)))
        {
            submit_navigation = self.apply_local_keyboard_activation(
                &mut document,
                id,
                &mut events,
                &mut history_commands,
            )?;
        }

        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native key event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        self.finish_local_keyboard_action(document, events, &history_commands, submit_navigation)
    }

    pub(super) fn action_local_key_sequence(
        &mut self,
        id: NativeNodeId,
        key: &str,
        modifiers: i64,
        apply_default: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        validate_native_key(key)?;
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let keydown_target = document.keyboard_event_target(id);
        let mut events = vec![(keydown_target, NativeEventKind::KeyDown)];
        let keyboard_button_target = document.has_native_keyboard_button_activation(id);
        let keyboard_checkable_kind = document.keyboard_checkable_kind(id);
        let keyboard_radio_arrow_target =
            document.can_apply_radio_group_arrow_navigation(id, key, modifiers);
        let keyboard_link_target =
            key == "Enter" && document.has_native_keyboard_link_activation(id);
        let keydown = self.evaluate_local_key_event_with_modifiers(
            &document,
            keydown_target,
            NativeEventKind::KeyDown,
            key,
            modifiers,
        )?;
        let keydown_allowed = keydown
            .as_ref()
            .and_then(|evaluation| evaluation.value.as_array())
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if let Some(keydown) = keydown {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &keydown.commands,
                &mut history_commands,
            )?);
        }

        if keydown_allowed && apply_default && should_apply_native_key_default(key, modifiers) {
            let default_events = if key == "Tab" {
                if document.focused_node() == id {
                    document.apply_tab_focus(modifiers & 8 != 0)?
                } else {
                    Vec::new()
                }
            } else if keyboard_radio_arrow_target {
                if document.can_apply_radio_group_arrow_navigation(id, key, modifiers) {
                    document.apply_key_default(id, key, modifiers)?
                } else {
                    Vec::new()
                }
            } else if document
                .focused_text_control()
                .is_ok_and(|focused| focused == id)
            {
                document.apply_key_default(id, key, modifiers)?
            } else {
                Vec::new()
            };
            events.extend(default_events.clone());
            for index in 0..default_events.len() {
                if let Some(evaluation) =
                    self.evaluate_local_transition_event(&document, &default_events, index)?
                {
                    events.extend(self.apply_local_evaluation_commands(
                        &mut document,
                        &evaluation.commands,
                        &mut history_commands,
                    )?);
                }
            }
        }

        let mut submit_navigation = None;
        if keydown_allowed
            && key == "Enter"
            && (keyboard_button_target || keyboard_link_target)
            && document.focused_node() == id
            && ((keyboard_button_target && document.has_native_keyboard_button_activation(id))
                || (keyboard_link_target && document.has_native_keyboard_link_activation(id)))
        {
            submit_navigation = self.apply_local_keyboard_activation(
                &mut document,
                id,
                &mut events,
                &mut history_commands,
            )?;
        }

        let keyup_target = document.keyboard_event_target(document.focused_node());
        events.push((keyup_target, NativeEventKind::KeyUp));
        if let Some(evaluation) = self.evaluate_local_key_event_with_modifiers(
            &document,
            keyup_target,
            NativeEventKind::KeyUp,
            key,
            modifiers,
        )? {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &evaluation.commands,
                &mut history_commands,
            )?);
        }
        let keyboard_checkable_target_remains = modifiers == 0
            && keyboard_checkable_kind
                .is_some_and(|expected| document.keyboard_checkable_kind(id) == Some(expected));
        if keydown_allowed
            && key == " "
            && document.focused_node() == id
            && ((keyboard_button_target && document.has_native_keyboard_button_activation(id))
                || keyboard_checkable_target_remains)
        {
            submit_navigation = self.apply_local_keyboard_activation(
                &mut document,
                id,
                &mut events,
                &mut history_commands,
            )?;
        }
        if key == " " {
            self.pending_space_activation = None;
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native key press effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }

        self.finish_local_keyboard_action(document, events, &history_commands, submit_navigation)
    }

    async fn apply_keyboard_mutation_async(
        &mut self,
        mutation: NativeContentMutation,
        keyboard_link_target: Option<NativeNodeId>,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let navigation = mutation.navigation.clone();
        let activated_link = keyboard_link_target.filter(|target| {
            mutation.allowed
                && mutation.events.iter().any(|event| {
                    event.node_index == target.index() && event.kind == NativeEventKind::Click
                })
        });
        let next_revision = self.next_revision()?;
        let outcome = self
            .apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        if let Some(navigation) = navigation {
            self.navigate_script_navigation_async(navigation, 0).await?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        if let Some(target) = activated_link {
            return self
                .finish_content_keyboard_link_activation_async(target, outcome)
                .await;
        }
        Ok(outcome)
    }

    async fn finish_content_keyboard_link_activation_async(
        &mut self,
        target: NativeNodeId,
        outcome: NativeActionResult,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let Some(href) = self
            .document
            .link_href(target)
            .filter(|href| !href.is_empty())
            .map(str::to_owned)
        else {
            return Ok(outcome);
        };
        let download_attribute = self
            .document
            .link_download_attribute(target)
            .map(str::to_owned);
        if download_attribute.is_some()
            && self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS
        {
            return Err(NativeEngineError::limit(
                "native pending downloads",
                MAX_NATIVE_PENDING_DOWNLOADS,
                self.pending_downloads.len().saturating_add(1),
            ));
        }
        let target_url = self.resolve_link_href(&href)?;
        let opens_new_target = self.document.link_opens_new_target(target);
        let special_navigation = download_attribute.is_some() || opens_new_target;
        if special_navigation
            && !self
                .allows_top_level_navigation_async(&target_url, true)
                .await?
        {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: outcome.accepted,
            });
        }
        if let Some(download_attribute) = download_attribute {
            self.queue_download(target_url, &download_attribute)?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: outcome.accepted,
            });
        }
        if opens_new_target {
            self.queue_popup(target_url)?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: outcome.accepted,
            });
        }
        self.navigate_request_async(NativeNavigationRequest::get(target_url), 0)
            .await?;
        Ok(NativeActionResult {
            revision: self.revision,
            accepted: outcome.accepted,
        })
    }
}
