use super::*;

impl NativeDocument {
    pub(crate) fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    /// Apply a bounded semantic click after the engine has resolved the target.
    /// Validation is completed before focus or control state is mutated.
    pub(crate) fn apply_click(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        self.apply_click_with_checkable_pre_activation(id, false)
    }

    pub(crate) fn apply_click_after_checkable_pre_activation(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        self.apply_click_with_checkable_pre_activation(id, true)
    }

    fn apply_click_with_checkable_pre_activation(
        &mut self,
        id: NativeNodeId,
        checkable_pre_activated: bool,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let Some(semantic) = self.semantic_node(id) else {
            if self.node(id).and_then(NativeNode::element_name) == Some("area")
                && self.image_map_for_area(id).is_some()
            {
                self.mark_user_interacted(id)?;
                return Ok(vec![(id, NativeEventKind::Click)]);
            }
            return Err(NativeEngineError::TargetNotActionable {
                reason: "target has no supported semantic control role".into(),
            });
        };
        let checkable_activation_role = matches!(semantic.role.as_str(), "checkbox" | "radio");
        if semantic.disabled && !(checkable_pre_activated && checkable_activation_role) {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.hidden && !(checkable_pre_activated && checkable_activation_role) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        if !matches!(
            semantic.role.as_str(),
            "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
        ) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "only supported semantic controls accept click".into(),
            });
        }
        let option_select_id = if semantic.role == "option" {
            Some(self.select_for_option(id)?)
        } else {
            None
        };

        let mut events = self.focus_element(id);
        self.mark_user_interacted(id)?;
        events.push((id, NativeEventKind::Click));
        match semantic.role.as_str() {
            "checkbox" => {
                if !checkable_pre_activated {
                    let node = self.node_mut(id).ok_or(NativeEngineError::DetachedTarget)?;
                    node.state.checked = !node.state.checked;
                    node.state.indeterminate = false;
                }
                events.push((id, NativeEventKind::Input));
                events.push((id, NativeEventKind::Change));
            }
            "radio" => {
                if !checkable_pre_activated {
                    let mut radio_ids = self.radio_group_members(id);
                    if !radio_ids.contains(&id) {
                        radio_ids.push(id);
                    }
                    for radio_id in radio_ids {
                        self.node_mut(radio_id)
                            .ok_or(NativeEngineError::DetachedTarget)?
                            .state
                            .checked = radio_id == id;
                    }
                }
                events.push((id, NativeEventKind::Input));
                events.push((id, NativeEventKind::Change));
            }
            "option" => {
                let select_id = option_select_id.ok_or(NativeEngineError::DetachedTarget)?;
                let option_ids = self.select_option_ids(select_id);
                let multiple = self
                    .node(select_id)
                    .is_some_and(|node| node.attribute("multiple").is_some());
                let mut selection_changed = false;
                for option_id in option_ids {
                    let should_be_selected = if multiple {
                        if option_id == id {
                            !self
                                .node(option_id)
                                .ok_or(NativeEngineError::DetachedTarget)?
                                .state
                                .selected
                        } else {
                            self.node(option_id)
                                .ok_or(NativeEngineError::DetachedTarget)?
                                .state
                                .selected
                        }
                    } else {
                        option_id == id
                    };
                    let was_selected = self
                        .node(option_id)
                        .ok_or(NativeEngineError::DetachedTarget)?
                        .state
                        .selected;
                    if was_selected != should_be_selected {
                        self.node_mut(option_id)
                            .ok_or(NativeEngineError::DetachedTarget)?
                            .state
                            .selected = should_be_selected;
                        selection_changed = true;
                    }
                }
                if selection_changed {
                    events.push((select_id, NativeEventKind::Change));
                }
            }
            _ => {}
        }
        Ok(events)
    }

    /// Deliver the pointer-enter events for one visible element and retain
    /// the hovered ancestor chain for CSS `:hover` matching.
    ///
    /// Pointer movement is represented as a semantic action rather than raw
    /// coordinates here. Layout hit testing and target routing are performed
    /// by the engine/backend; the document owns the DOM event order.
    pub(crate) fn apply_hover(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name().is_none() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hover requires an element target".into(),
            });
        }
        if self.is_hidden(id) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        if !self
            .computed_style_for_layout(id)
            .pointer_events()
            .allows_hit_testing()
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "target does not accept pointer events".into(),
            });
        }
        for node in &mut self.nodes {
            node.state.hovered = false;
        }
        let mut current = Some(id);
        for _ in 0..=MAX_NATIVE_DOM_DEPTH {
            let Some(current_id) = current else {
                break;
            };
            let Some(node) = self.node_mut(current_id) else {
                break;
            };
            node.state.hovered = true;
            current = node.parent();
        }
        Ok(vec![
            (id, NativeEventKind::MouseOver),
            (id, NativeEventKind::MouseEnter),
        ])
    }

    /// Deliver the DOM drag event sequence for one visible source and target.
    ///
    /// The current native action is semantic, so it intentionally does not
    /// synthesize coordinates or a `DataTransfer` object. It does preserve the
    /// browser-visible event ordering and bubbling/cancellation metadata.
    pub(crate) fn apply_drag(
        &self,
        source: NativeNodeId,
        destination: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        for (label, id) in [("drag source", source), ("drag destination", destination)] {
            let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
            if node.element_name().is_none() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!("{label} requires an element target"),
                });
            }
            if self.is_hidden(id) {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!("hidden {label} is not actionable"),
                });
            }
            if !self
                .computed_style_for_layout(id)
                .pointer_events()
                .allows_hit_testing()
            {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: format!("{label} does not accept pointer events"),
                });
            }
        }
        Ok(vec![
            (source, NativeEventKind::DragStart),
            (destination, NativeEventKind::DragEnter),
            (destination, NativeEventKind::DragOver),
            (destination, NativeEventKind::Drop),
            (source, NativeEventKind::DragEnd),
        ])
    }

    /// Attach bounded file objects to one native file input and return the
    /// browser-visible input/change event order. File selection is a direct
    /// browser-owner operation, so it remains valid for hidden file inputs;
    /// the file contents never become DOM attributes or locator text.
    pub(crate) fn apply_upload(
        &mut self,
        id: NativeNodeId,
        files: &[NativeFile],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        NativeFile::validate_many(files)?;
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name() != Some("input")
            || !node
                .attribute("type")
                .unwrap_or("text")
                .eq_ignore_ascii_case("file")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "upload requires an input type=file control".into(),
            });
        }
        if self.is_disabled(id) {
            return Err(NativeEngineError::DisabledTarget);
        }
        if node.attribute("multiple").is_none() && files.len() > 1 {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "file input does not allow multiple files".into(),
            });
        }
        let fake_path = format!(r"C:\fakepath\{}", files[0].name);
        {
            let state = &mut self
                .node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state;
            state.files = files.to_vec();
        }
        self.mark_user_interacted(id)?;
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .value = Some(fake_path);
        Ok(vec![
            (id, NativeEventKind::Input),
            (id, NativeEventKind::Change),
        ])
    }

    /// Replace a supported text control's value with bounded input text.
    /// The value itself stays private; semantic projections expose only state.
    pub(crate) fn apply_type(
        &mut self,
        id: NativeNodeId,
        text: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported semantic text-control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        if semantic.role != "textbox" || !matches!(semantic.tag_name.as_str(), "input" | "textarea")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "type requires an input or textarea textbox".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.read_only {
            return Err(NativeEngineError::ReadOnlyTarget);
        }
        if text.is_empty() {
            return Err(NativeEngineError::invalid(
                "action text",
                "must not be empty",
            ));
        }
        if text.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "action text",
                MAX_LOCATOR_BYTES,
                text.len(),
            ));
        }

        let mut events = self.focus_element(id);
        self.mark_user_interacted(id)?;
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .value = Some(text.to_owned());
        self.set_selection_state(id, text.chars().count(), text.chars().count(), "none")?;
        events.push((id, NativeEventKind::Input));
        events.push((id, NativeEventKind::Change));
        Ok(events)
    }

    /// Clear a supported editable control and return its native event order.
    /// An already-empty control remains a successful, focus-preserving no-op.
    pub(crate) fn apply_clear(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported semantic text-control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot be cleared".into(),
            });
        }
        if semantic.role != "textbox" || !matches!(semantic.tag_name.as_str(), "input" | "textarea")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "clear requires an input or textarea textbox".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.read_only {
            return Err(NativeEngineError::ReadOnlyTarget);
        }

        let mut events = self.focus_element(id);
        self.mark_user_interacted(id)?;
        let current = self.current_value(id).unwrap_or_default();
        if !current.is_empty() {
            self.node_mut(id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .value = Some(String::new());
            self.set_selection_state(id, 0, 0, "none")?;
            events.push((id, NativeEventKind::Input));
            events.push((id, NativeEventKind::Change));
        }
        Ok(events)
    }

    /// Return the checked state of one visible, enabled checkbox or radio.
    pub(crate) fn checked_control_state(
        &self,
        id: NativeNodeId,
    ) -> Result<bool, NativeEngineError> {
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported checkable control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot be checked".into(),
            });
        }
        if !matches!(semantic.role.as_str(), "checkbox" | "radio") || semantic.tag_name != "input" {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "check and uncheck require a checkbox or radio input".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        Ok(semantic.checked.unwrap_or(false))
    }

    /// Select one exact option value and return its native event order.
    pub(crate) fn apply_select(
        &mut self,
        id: NativeNodeId,
        value: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        if value.is_empty() || value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::invalid(
                "select value",
                "must be 1..=16384 bytes",
            ));
        }
        let semantic =
            self.semantic_node(id)
                .ok_or_else(|| NativeEngineError::TargetNotActionable {
                    reason: "target has no supported select control role".into(),
                })?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot be selected".into(),
            });
        }
        if semantic.role != "combobox" || semantic.tag_name != "select" {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "select requires a select control".into(),
            });
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        let selected_option_id = self
            .select_option_ids(id)
            .into_iter()
            .find(|option_id| {
                self.option_value(*option_id).as_deref() == Some(value)
                    && !self.is_disabled(*option_id)
            })
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "select option value was not found or is disabled".into(),
            })?;

        let mut events = self.focus_element(id);
        self.mark_user_interacted(id)?;
        let option_ids = self.select_option_ids(id);
        let mut changed = false;
        for option_id in option_ids {
            let should_be_selected = option_id == selected_option_id;
            let was_selected = self
                .node(option_id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .selected;
            if was_selected != should_be_selected {
                self.node_mut(option_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .selected = should_be_selected;
                changed = true;
            }
        }
        if changed {
            events.push((id, NativeEventKind::Input));
            events.push((id, NativeEventKind::Change));
        }
        Ok(events)
    }

    pub(crate) fn focused_text_control(&self) -> Result<NativeNodeId, NativeEngineError> {
        self.nodes
            .iter()
            .find(|node| {
                self.is_attached(node.id())
                    && node.state.focused
                    && matches!(node.element_name(), Some("input" | "textarea"))
                    && self
                        .semantic_node(node.id())
                        .is_some_and(|semantic| semantic.role == "textbox")
            })
            .map(NativeNode::id)
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "key press requires a focused text control".into(),
            })
    }

    /// Return the active page target for a raw key event. Browsers deliver
    /// keyboard events to the focused element and otherwise to the document;
    /// the native host uses the document root as that no-focus target.
    pub(crate) fn focused_node(&self) -> NativeNodeId {
        self.nodes
            .iter()
            .find(|node| self.is_attached(node.id()) && node.state.focused)
            .map(NativeNode::id)
            .unwrap_or(self.root)
    }

    pub(crate) fn can_apply_radio_group_arrow_navigation(
        &self,
        id: NativeNodeId,
        key: &str,
        modifiers: i64,
    ) -> bool {
        modifiers == 0
            && matches!(key, "ArrowRight" | "ArrowDown" | "ArrowLeft" | "ArrowUp")
            && self.focused_node() == id
            && self.is_attached(id)
            && !self.is_disabled(id)
            && self.connected_checkable_kind(id) == Some(NativeCheckableKind::Radio)
            && !self.radio_group_members(id).is_empty()
    }

    /// Move focus through the bounded sequentially focusable controls in
    /// document order. Positive `tabindex` values are ordered before the
    /// natural zero-order controls; negative values are skipped.
    pub(crate) fn apply_tab_focus(
        &mut self,
        reverse: bool,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let mut tree_order = Vec::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            let Some(node) = self.node(id) else {
                continue;
            };
            tree_order.push(id);
            pending.extend(node.children().iter().rev().copied());
        }
        let mut order_by_node = vec![usize::MAX; self.nodes.len()];
        for (order, id) in tree_order.iter().enumerate() {
            if let Ok(index) = usize::try_from(id.index())
                && let Some(node_order) = order_by_node.get_mut(index)
            {
                *node_order = order;
            }
        }
        let mut focusable = Vec::new();
        for (order, id) in tree_order.iter().copied().enumerate() {
            let Some(node) = self.node(id) else {
                continue;
            };
            if !self.is_attached(id) || self.is_inert(id) {
                continue;
            }
            let explicit_tab_index = node.attribute("tabindex").and_then(parse_native_tab_index);
            if node.element_name() == Some("area") {
                let Some(role) = self.semantic_role(id) else {
                    continue;
                };
                if self.is_disabled(id)
                    || !matches!(
                        role,
                        "button" | "link" | "textbox" | "checkbox" | "radio" | "combobox"
                    )
                    || self.is_hidden(id)
                {
                    continue;
                }
                let tab_index = explicit_tab_index.unwrap_or(0);
                if tab_index < 0 {
                    continue;
                }
                for anchor in self.image_map_area_focus_anchors(id) {
                    if self.is_inert(anchor) {
                        continue;
                    }
                    let Some(anchor_order) = usize::try_from(anchor.index())
                        .ok()
                        .and_then(|index| order_by_node.get(index))
                        .copied()
                        .filter(|order| *order != usize::MAX)
                    else {
                        continue;
                    };
                    focusable.push((tab_index, anchor_order, order, id, Some(anchor)));
                }
                continue;
            }
            if let Some(tab_index) = explicit_tab_index {
                if tab_index >= 0 && self.can_focus_explicit_tabindex(id) {
                    focusable.push((tab_index, order, order, id, None));
                }
                continue;
            }
            if self.is_disabled(id) || self.is_hidden(id) {
                continue;
            }
            let Some(role) = self.semantic_role(id) else {
                continue;
            };
            if matches!(
                role,
                "button" | "link" | "textbox" | "checkbox" | "radio" | "combobox"
            ) {
                focusable.push((0, order, order, id, None));
            }
        }
        focusable.sort_by_key(|(tab_index, anchor_order, element_order, _, _)| {
            (
                if *tab_index > 0 { 0 } else { 1 },
                (*tab_index).max(0),
                *anchor_order,
                *element_order,
            )
        });
        let Some((current_index, _)) =
            focusable
                .iter()
                .enumerate()
                .find_map(|(index, (_, _, _, id, anchor))| {
                    self.node(*id)
                        .is_some_and(|node| {
                            node.state.focused
                                && node.state.focus_anchor_node_index
                                    == anchor.map(NativeNodeId::index)
                        })
                        .then_some((index, *id))
                })
        else {
            let Some((_, _, _, id, anchor)) = (if reverse {
                focusable.last().copied()
            } else {
                focusable.first().copied()
            }) else {
                return Ok(Vec::new());
            };
            return Ok(self.focus_element_with_anchor(id, anchor));
        };
        let next_index = if reverse {
            current_index.checked_sub(1).unwrap_or(focusable.len() - 1)
        } else {
            (current_index + 1) % focusable.len()
        };
        let (_, _, _, id, anchor) = focusable[next_index];
        Ok(self.focus_element_with_anchor(id, anchor))
    }

    /// Return the bounded text-control selection as character offsets and a
    /// direction. Non-text elements have no selection API in the native host.
    pub(super) fn selection_snapshot(&self, id: NativeNodeId) -> Option<(usize, usize, String)> {
        let node = self.node(id)?;
        if !matches!(node.element_name(), Some("input" | "textarea"))
            || !self
                .semantic_node(id)
                .is_some_and(|semantic| semantic.role == "textbox")
        {
            return None;
        }
        let length = self.current_value(id).unwrap_or_default().chars().count();
        let start = node.state.selection_start.unwrap_or(0).min(length);
        let end = node.state.selection_end.unwrap_or(start).min(length);
        Some((
            start.min(end),
            start.max(end),
            node.state
                .selection_direction
                .clone()
                .unwrap_or_else(|| "none".into()),
        ))
    }

    pub(super) fn set_selection_state(
        &mut self,
        id: NativeNodeId,
        start: usize,
        end: usize,
        direction: &str,
    ) -> Result<(), NativeEngineError> {
        if !matches!(
            self.node(id).and_then(NativeNode::element_name),
            Some("input" | "textarea")
        ) || !self
            .semantic_node(id)
            .is_some_and(|semantic| semantic.role == "textbox")
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "selection requires an input or textarea".into(),
            });
        }
        if !matches!(direction, "none" | "forward" | "backward") {
            return Err(NativeEngineError::invalid(
                "selection direction",
                "must be none, forward, or backward",
            ));
        }
        let length = self.current_value(id).unwrap_or_default().chars().count();
        let start = start.min(length);
        let end = end.min(length);
        let (start, end) = (start.min(end), start.max(end));
        let node = self.node_mut(id).ok_or(NativeEngineError::DetachedTarget)?;
        node.state.selection_start = Some(start);
        node.state.selection_end = Some(end);
        node.state.selection_direction = Some(direction.to_owned());
        Ok(())
    }

    pub(super) fn initialize_selection_if_needed(&mut self, id: NativeNodeId) {
        let Some((length, needs_initialization)) = self.node(id).and_then(|node| {
            matches!(node.element_name(), Some("input" | "textarea")).then(|| {
                (
                    self.current_value(id).unwrap_or_default().chars().count(),
                    node.state.selection_start.is_none() || node.state.selection_end.is_none(),
                )
            })
        }) else {
            return;
        };
        if needs_initialization && let Some(node) = self.node_mut(id) {
            node.state.selection_start = Some(length);
            node.state.selection_end = Some(length);
            node.state.selection_direction = Some("none".into());
        }
    }

    /// Apply the bounded default action for one keyboard sequence. The
    /// returned input event is present only when the value changed; selection
    /// and caret movement themselves remain silent DOM state changes.
    pub(crate) fn apply_key_default(
        &mut self,
        id: NativeNodeId,
        key: &str,
        modifiers: i64,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        validate_native_key(key)?;
        let semantic = self
            .semantic_node(id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if semantic.hidden {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets cannot receive key input".into(),
            });
        }
        if semantic.role == "radio" && semantic.tag_name == "input" {
            if semantic.disabled {
                return Err(NativeEngineError::DisabledTarget);
            }
            if self.focused_node() != id {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "key default target is not focused".into(),
                });
            }
            if matches!(key, "ArrowRight" | "ArrowDown" | "ArrowLeft" | "ArrowUp") && modifiers == 0
            {
                return self.apply_radio_group_arrow_default(id, key);
            }
            return Ok(Vec::new());
        }
        if semantic.role != "textbox" || !matches!(semantic.tag_name.as_str(), "input" | "textarea")
        {
            return Ok(Vec::new());
        }
        if semantic.disabled {
            return Err(NativeEngineError::DisabledTarget);
        }
        if semantic.read_only {
            return Err(NativeEngineError::ReadOnlyTarget);
        }
        if !self.node(id).is_some_and(|node| node.state.focused) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "key default target is not focused".into(),
            });
        }
        self.mark_user_interacted(id)?;
        self.initialize_selection_if_needed(id);
        let (start, end, direction) = self
            .selection_snapshot(id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        let value = self.current_value(id).unwrap_or_default();
        let characters = value.chars().collect::<Vec<_>>();
        let length = characters.len();
        let primary_modifier = modifiers & (2 | 4) != 0;
        let shift = modifiers & 8 != 0;
        let alt = modifiers & 1 != 0;

        if primary_modifier && key.eq_ignore_ascii_case("a") {
            self.set_selection_state(id, 0, length, "forward")?;
            return Ok(Vec::new());
        }
        if primary_modifier || alt {
            return Ok(Vec::new());
        }

        if matches!(key, "ArrowLeft" | "ArrowRight" | "Home" | "End") {
            let current_focus = if direction == "backward" { start } else { end };
            let focus = match key {
                "ArrowLeft" => current_focus.saturating_sub(1),
                "ArrowRight" => (current_focus + 1).min(length),
                "Home" => 0,
                "End" => length,
                _ => unreachable!("matched edge key"),
            };
            if shift {
                let anchor = if direction == "backward" { end } else { start };
                let direction = if focus < anchor {
                    "backward"
                } else if focus > anchor {
                    "forward"
                } else {
                    "none"
                };
                self.set_selection_state(id, anchor, focus, direction)?;
            } else {
                let caret = if start != end {
                    if matches!(key, "ArrowLeft" | "Home") {
                        start
                    } else {
                        end
                    }
                } else {
                    focus
                };
                self.set_selection_state(id, caret, caret, "none")?;
            }
            return Ok(Vec::new());
        }

        let (next_value, next_caret) = if key.chars().count() == 1 {
            let mut next = Vec::with_capacity(length + 1);
            next.extend_from_slice(&characters[..start]);
            next.extend(key.chars());
            next.extend_from_slice(&characters[end..]);
            (next.into_iter().collect::<String>(), start + 1)
        } else if key == "Backspace" {
            let remove_start = if start != end {
                start
            } else {
                start.saturating_sub(1)
            };
            let remove_end = if start != end { end } else { start };
            if remove_start == remove_end {
                self.set_selection_state(id, start, start, "none")?;
                return Ok(Vec::new());
            }
            let mut next = Vec::with_capacity(length);
            next.extend_from_slice(&characters[..remove_start]);
            next.extend_from_slice(&characters[remove_end..]);
            (next.into_iter().collect::<String>(), remove_start)
        } else if key == "Delete" {
            let remove_start = start;
            let remove_end = if start != end {
                end
            } else {
                (start + 1).min(length)
            };
            if remove_start == remove_end {
                self.set_selection_state(id, start, start, "none")?;
                return Ok(Vec::new());
            }
            let mut next = Vec::with_capacity(length);
            next.extend_from_slice(&characters[..remove_start]);
            next.extend_from_slice(&characters[remove_end..]);
            (next.into_iter().collect::<String>(), remove_start)
        } else {
            return Ok(Vec::new());
        };
        if next_value.len() > MAX_LOCATOR_BYTES {
            return Err(NativeEngineError::limit(
                "native text value",
                MAX_LOCATOR_BYTES,
                next_value.len(),
            ));
        }
        self.node_mut(id)
            .ok_or(NativeEngineError::DetachedTarget)?
            .state
            .value = Some(next_value);
        self.set_selection_state(id, next_caret, next_caret, "none")?;
        Ok(vec![(id, NativeEventKind::Input)])
    }

    fn apply_radio_group_arrow_default(
        &mut self,
        id: NativeNodeId,
        key: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        if !self.can_apply_radio_group_arrow_navigation(id, key, 0) {
            return Ok(Vec::new());
        }
        let group = self.radio_group_members(id);
        let Some(current_index) = group.iter().position(|member| *member == id) else {
            return Ok(Vec::new());
        };
        let forward = matches!(key, "ArrowRight" | "ArrowDown");
        let Some(destination) = (1..=group.len()).find_map(|step| {
            let index = if forward {
                (current_index + step) % group.len()
            } else {
                (current_index + group.len() - (step % group.len())) % group.len()
            };
            let candidate = group[index];
            (!self.is_disabled(candidate)).then_some(candidate)
        }) else {
            return Ok(Vec::new());
        };
        let selection_changed = !self
            .node(destination)
            .is_some_and(|node| node.state.checked);
        for radio_id in group {
            self.node_mut(radio_id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .checked = radio_id == destination;
        }
        self.mark_user_interacted(destination)?;
        let mut events = self.focus_element(destination);
        if selection_changed {
            events.push((destination, NativeEventKind::Input));
            events.push((destination, NativeEventKind::Change));
        }
        Ok(events)
    }

    /// Apply the bounded default edit for one key to a focused text control.
    /// Selection, caret movement, composition, and form submission are kept
    /// out of this slice; printable keys append at the current value end.
    pub(crate) fn apply_key_press(
        &mut self,
        id: NativeNodeId,
        key: &str,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        validate_native_edit_key(key)?;
        self.apply_key_default(id, key, 0)
    }

    /// Apply validated commands emitted by one JavaScript evaluation.
    /// Commands run against the caller's document clone so the engine can
    /// commit the complete script batch as one revision.
    pub(crate) fn apply_script_commands(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        self.apply_script_commands_with_link_policy(commands, false)
    }

    pub(crate) fn apply_script_commands_allowing_links(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        self.apply_script_commands_with_link_policy(commands, true)
    }

    fn apply_script_commands_with_link_policy(
        &mut self,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        let mut events = Vec::new();
        let mut script_nodes = self.script_node_ids.clone();
        for command in commands {
            match command {
                NativeScriptCommand::AdoptNode { .. } => {
                    return Err(NativeEngineError::invalid(
                        "native node adoption",
                        "cross-context adoption must use the paired browsing-context transfer path",
                    ));
                }
                NativeScriptCommand::AdoptedNodeCommand { .. } => {
                    return Err(NativeEngineError::invalid(
                        "adopted native node command",
                        "must be routed through the browsing-context coordinator",
                    ));
                }
                NativeScriptCommand::Focus { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    events.extend(self.apply_script_focus(id)?);
                }
                NativeScriptCommand::Blur { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    events.extend(self.apply_script_blur(id)?);
                }
                NativeScriptCommand::Click { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    if !allow_script_navigation
                        && self.link_href(id).is_some_and(|href| !href.is_empty())
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-driven link navigation is not available".into(),
                        });
                    }
                    events.extend(self.apply_click(id)?);
                }
                NativeScriptCommand::SubmitForm { node_index } => {
                    if !allow_script_navigation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "script-driven form submission is not available in this event phase"
                                    .into(),
                        });
                    }
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    if self
                        .node(id)
                        .is_none_or(|node| node.element_name() != Some("form"))
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script form submission target is not a form".into(),
                        });
                    }
                }
                NativeScriptCommand::RequestSubmitForm {
                    node_index,
                    submitter_index,
                } => {
                    if !allow_script_navigation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "script-driven form submission is not available in this event phase"
                                    .into(),
                        });
                    }
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    if self
                        .node(id)
                        .is_none_or(|node| node.element_name() != Some("form"))
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script form submission target is not a form".into(),
                        });
                    }
                    if let Some(submitter_index) = submitter_index {
                        let submitter = NativeNodeId::from_parts(self.generation, *submitter_index);
                        if self.submit_control_form(submitter) != Some(id) {
                            return Err(NativeEngineError::TargetNotActionable {
                                reason:
                                    "requestSubmit submitter must be a submit control for the form"
                                        .into(),
                            });
                        }
                    }
                }
                NativeScriptCommand::ResetForm {
                    node_index,
                    perform_reset,
                } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    if self
                        .node(id)
                        .is_none_or(|node| node.element_name() != Some("form"))
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script form reset target is not a form".into(),
                        });
                    }
                    if *perform_reset {
                        self.reset_form_controls(id)?;
                    }
                    events.push((id, NativeEventKind::Reset));
                }
                NativeScriptCommand::Fetch { .. } => {}
                NativeScriptCommand::FontFaceInstall { .. } => {
                    self.apply_script_font_face_install(command)?;
                }
                NativeScriptCommand::ServiceWorkerRegister { .. }
                | NativeScriptCommand::ServiceWorkerUnregister { .. }
                | NativeScriptCommand::ServiceWorkerUpdate { .. }
                | NativeScriptCommand::ServiceWorkerNavigationPreload { .. }
                | NativeScriptCommand::ServiceWorkerSkipWaiting { .. }
                | NativeScriptCommand::ServiceWorkerClientsClaim { .. }
                | NativeScriptCommand::ServiceWorkerPostMessage { .. }
                | NativeScriptCommand::ServiceWorkerClientPostMessage { .. }
                | NativeScriptCommand::ServiceWorkerCacheOpen { .. }
                | NativeScriptCommand::ServiceWorkerCacheDelete { .. }
                | NativeScriptCommand::ServiceWorkerCacheHas { .. }
                | NativeScriptCommand::ServiceWorkerCacheKeys { .. }
                | NativeScriptCommand::ServiceWorkerCacheMatch { .. }
                | NativeScriptCommand::ServiceWorkerCachePut { .. }
                | NativeScriptCommand::ServiceWorkerCachePutAll { .. }
                | NativeScriptCommand::ServiceWorkerCacheDeleteRequest { .. }
                | NativeScriptCommand::ServiceWorkerCacheEntries { .. }
                | NativeScriptCommand::ServiceWorkerOpenWindow { .. }
                | NativeScriptCommand::MessagePortPostMessage { .. }
                | NativeScriptCommand::MessagePortClose { .. } => {}
                NativeScriptCommand::SharedWorkerCreate { .. }
                | NativeScriptCommand::WorkerCreate { .. }
                | NativeScriptCommand::WorkerPostMessage { .. }
                | NativeScriptCommand::WorkerTerminate { .. }
                | NativeScriptCommand::WorkerClose { .. } => {}
                NativeScriptCommand::WorkerScriptError { .. } => {
                    return Err(NativeEngineError::invalid(
                        "native Worker script error",
                        "must be routed through the dedicated Worker host boundary",
                    ));
                }
                NativeScriptCommand::WebSocketOpen { .. }
                | NativeScriptCommand::WebSocketSend { .. }
                | NativeScriptCommand::WebSocketClose { .. }
                | NativeScriptCommand::EventSourceOpen { .. }
                | NativeScriptCommand::EventSourceClose { .. }
                | NativeScriptCommand::FetchStreamRead { .. }
                | NativeScriptCommand::FetchStreamCancel { .. }
                | NativeScriptCommand::FetchUploadChunk { .. }
                | NativeScriptCommand::FetchUploadEnd { .. }
                | NativeScriptCommand::FetchUploadError { .. }
                | NativeScriptCommand::FetchUploadCancel { .. } => {}
                NativeScriptCommand::Navigate { .. } => {
                    if !allow_script_navigation {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "script location navigation is not available in this event phase"
                                    .into(),
                        });
                    }
                }
                NativeScriptCommand::HistoryPushState { .. }
                | NativeScriptCommand::HistoryReplaceState { .. }
                | NativeScriptCommand::HistoryGo { .. }
                | NativeScriptCommand::ScrollTo { .. } => {}
                NativeScriptCommand::StorageSet { .. }
                | NativeScriptCommand::StorageRemove { .. }
                | NativeScriptCommand::StorageClear { .. }
                | NativeScriptCommand::CookieSet { .. }
                | NativeScriptCommand::Dialog { .. }
                | NativeScriptCommand::OpenWindow { .. }
                | NativeScriptCommand::SetWindowName { .. }
                | NativeScriptCommand::CloseWindow { .. }
                | NativeScriptCommand::NavigateWindow { .. }
                | NativeScriptCommand::PostMessage { .. }
                | NativeScriptCommand::FrameScriptBatch { .. }
                | NativeScriptCommand::FrameScript { .. } => {}
                NativeScriptCommand::ClearFileInput { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    let node = self
                        .script_node(id, &script_nodes)
                        .ok_or(NativeEngineError::DetachedTarget)?;
                    if node.element_name() != Some("input")
                        || !node
                            .attribute("type")
                            .unwrap_or("text")
                            .eq_ignore_ascii_case("file")
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "clear file input requires an input type=file control".into(),
                        });
                    }
                    let node = self
                        .script_node_mut(id, &script_nodes)
                        .ok_or(NativeEngineError::DetachedTarget)?;
                    node.state.files.clear();
                    node.state.value = Some(String::new());
                }
                NativeScriptCommand::SetValue { node_index, value } => {
                    self.apply_script_value(*node_index, value, &script_nodes)?;
                }
                NativeScriptCommand::SetSelection {
                    node_index,
                    start,
                    end,
                    direction,
                } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    self.set_selection_state(id, *start, *end, direction)?;
                }
                NativeScriptCommand::CanvasCommit {
                    node_index,
                    width,
                    height,
                    pixels_base64,
                    origin_clean,
                } => {
                    self.apply_script_canvas(
                        *node_index,
                        *width,
                        *height,
                        pixels_base64,
                        *origin_clean,
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::MediaLoad { node_index } => {
                    self.reset_media_load(*node_index, &script_nodes)?;
                }
                NativeScriptCommand::SetChecked {
                    node_index,
                    checked,
                } => {
                    self.apply_script_checked(*node_index, *checked, &script_nodes)?;
                }
                NativeScriptCommand::SetIndeterminate {
                    node_index,
                    indeterminate,
                } => {
                    self.apply_script_indeterminate(*node_index, *indeterminate, &script_nodes)?;
                }
                NativeScriptCommand::SetSelected {
                    node_index,
                    selected,
                } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    self.apply_script_selected(id, *selected)?;
                }
                NativeScriptCommand::SetAttribute {
                    node_index,
                    name,
                    value,
                    namespace_uri,
                } => {
                    self.apply_script_attribute(
                        *node_index,
                        name,
                        value,
                        namespace_uri.as_deref(),
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::RemoveAttribute {
                    node_index,
                    name,
                    namespace_uri,
                } => {
                    self.remove_script_attribute(
                        *node_index,
                        name,
                        namespace_uri.as_deref(),
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::SetTextContent { node_index, value } => {
                    self.apply_script_text_content(*node_index, value, &script_nodes)?;
                }
                NativeScriptCommand::SetDocumentTitle { value } => {
                    self.apply_script_document_title(value)?;
                }
                NativeScriptCommand::SetInnerHtml { node_index, value } => {
                    self.apply_script_inner_html(*node_index, value, &script_nodes)?;
                }
                NativeScriptCommand::BindInnerHtmlNodeIndexes {
                    root_index,
                    temporary_node_indexes,
                } => {
                    let root_id = self.resolve_script_node_id(*root_index, &script_nodes);
                    self.bind_inner_html_node_indexes(
                        root_id,
                        temporary_node_indexes,
                        &mut script_nodes,
                    )?;
                }
                NativeScriptCommand::RemoveNode { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    self.apply_script_remove_node(id, &script_nodes)?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateElement {
                    node_index,
                    tag_name,
                    namespace_uri,
                } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    let name = validate_script_element_name(tag_name)?;
                    let namespace_uri = match namespace_uri.as_deref() {
                        None => Some(HTML_NAMESPACE_URI.to_owned()),
                        Some("") => None,
                        Some(value) => Some(validate_namespace_uri(value)?),
                    };
                    let has_template_contents =
                        name == "template" && namespace_uri.as_deref() == Some(HTML_NAMESPACE_URI);
                    let id = self.add_detached_node(
                        NativeNodeKind::Element {
                            name,
                            attributes: BTreeMap::new(),
                        },
                        self.max_nodes,
                    )?;
                    if let Some(node) = self.raw_node_mut(id) {
                        node.state.namespace_uri = namespace_uri;
                    }
                    if has_template_contents {
                        let fragment = self
                            .add_detached_node(NativeNodeKind::DocumentFragment, self.max_nodes)?;
                        if let Some(template) = self.raw_node_mut(id) {
                            template.template_content = Some(fragment);
                        }
                        if let Some(fragment_node) = self.raw_node_mut(fragment) {
                            fragment_node.template_host = Some(id);
                        }
                    }
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateTextNode { node_index, value } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    if value.len() > MAX_LOCATOR_BYTES {
                        return Err(NativeEngineError::limit(
                            "script text node",
                            MAX_LOCATOR_BYTES,
                            value.len(),
                        ));
                    }
                    let id = self
                        .add_detached_node(NativeNodeKind::Text(value.clone()), self.max_nodes)?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateComment { node_index, value } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    if value.len() > MAX_LOCATOR_BYTES {
                        return Err(NativeEngineError::limit(
                            "script comment node",
                            MAX_LOCATOR_BYTES,
                            value.len(),
                        ));
                    }
                    let id = self.add_detached_node(
                        NativeNodeKind::Comment(value.clone()),
                        self.max_nodes,
                    )?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::CreateDocumentType {
                    node_index,
                    name,
                    public_id,
                    system_id,
                } => {
                    if *node_index < SCRIPT_TEMP_NODE_BASE || script_nodes.contains_key(node_index)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-created node index is invalid".into(),
                        });
                    }
                    let name = validate_script_element_name(name)?;
                    for (label, value) in [
                        ("public identifier", public_id),
                        ("system identifier", system_id),
                    ] {
                        if value.len() > MAX_LOCATOR_BYTES {
                            return Err(NativeEngineError::limit(
                                format!("script document type {label}"),
                                MAX_LOCATOR_BYTES,
                                value.len(),
                            ));
                        }
                    }
                    let id = self.add_detached_node(
                        NativeNodeKind::DocumentType {
                            name,
                            public_id: (!public_id.is_empty()).then(|| public_id.clone()),
                            system_id: (!system_id.is_empty()).then(|| system_id.clone()),
                        },
                        self.max_nodes,
                    )?;
                    script_nodes.insert(*node_index, id);
                }
                NativeScriptCommand::AppendChild {
                    parent_index,
                    child_index,
                } => {
                    let parent = self.resolve_script_node_id(*parent_index, &script_nodes);
                    let child = self.resolve_script_node_id(*child_index, &script_nodes);
                    if self.script_node(parent, &script_nodes).is_none() {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: format!(
                                "appendChild parent script node {parent_index} is not present in the current document generation"
                            ),
                        });
                    }
                    if self.script_node(child, &script_nodes).is_none() {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: format!(
                                "appendChild child script node {child_index} is not present in the current document generation"
                            ),
                        });
                    }
                    self.append_script_child(parent, child, None, &script_nodes)?;
                }
                NativeScriptCommand::InsertBefore {
                    parent_index,
                    child_index,
                    before_index,
                } => {
                    let parent = self.resolve_script_node_id(*parent_index, &script_nodes);
                    let child = self.resolve_script_node_id(*child_index, &script_nodes);
                    let before =
                        before_index.map(|index| self.resolve_script_node_id(index, &script_nodes));
                    self.append_script_child(parent, child, before, &script_nodes)?;
                }
                NativeScriptCommand::StartScript { node_index } => {
                    let id = self.resolve_script_node_id(*node_index, &script_nodes);
                    if self
                        .script_node(id, &script_nodes)
                        .and_then(NativeNode::element_name)
                        != Some("script")
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "script-start marker must target a script element".into(),
                        });
                    }
                }
                NativeScriptCommand::SetCustomValidity {
                    node_index,
                    message,
                } => {
                    self.apply_script_custom_validity(*node_index, message, &script_nodes)?;
                }
                NativeScriptCommand::SetFormAssociatedDisabledState {
                    node_index,
                    disabled,
                } => {
                    self.apply_script_form_associated_disabled_state(
                        *node_index,
                        *disabled,
                        &script_nodes,
                    )?;
                }
                NativeScriptCommand::CheckValidity { node_index }
                | NativeScriptCommand::ReportValidity { node_index } => {
                    let id = NativeNodeId::from_parts(self.generation, *node_index);
                    events.extend(self.validation_events(id)?);
                }
            }
            if events.len() > crate::native_engine::interaction::MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "script mutation effects",
                    crate::native_engine::interaction::MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        self.script_node_ids = script_nodes
            .into_iter()
            .filter(|(node_index, _)| *node_index >= SCRIPT_TEMP_NODE_BASE)
            .collect();
        Ok(events)
    }
}
