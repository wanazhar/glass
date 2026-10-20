use super::*;

pub(super) fn mutate_click_with_event_preflight(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    modifiers: u8,
    mut loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = next.apply_script_focus(node_id)?;
    let focus_metadata = events
        .iter()
        .map(|(node, kind)| (node.index(), *kind))
        .collect::<Vec<_>>();
    if let Some(event_batch) = host_event_batch(&focus_metadata)? {
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            document_url,
            document_origin,
            viewport,
        )?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        let (effects, _) = apply_document_commands_with_font_face_ack(
            &mut next,
            runtime,
            document_url,
            document_origin,
            viewport,
            &evaluation.commands,
            false,
        )?;
        events.extend(effects);
    }

    let checkable_pre_activation = next.pre_activate_checkable(node_id)?;

    let click_event_batch = host_click_event_batch_with_modifiers(node_index, modifiers)?;
    let click_evaluation = runtime.evaluate_with_host_events(
        &click_event_batch,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    let click_allowed = click_evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process click preflight".into(),
            reason: "native click event result was invalid".into(),
        })?;
    apply_content_event_history(
        &click_evaluation.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&click_evaluation.commands));
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &click_evaluation.commands,
        false,
    )?;
    events.extend(effects);
    let mut navigation = None;
    if click_allowed {
        let click_events = if checkable_pre_activation.is_some() {
            next.apply_click_after_checkable_pre_activation(node_id)?
        } else {
            next.apply_click(node_id)?
        };
        let dispatch_checkable_events =
            checkable_pre_activation.is_some() && next.connected_checkable_kind(node_id).is_some();
        events.extend(click_events);
        if dispatch_checkable_events {
            let event_batch = host_event_batch(&[
                (node_index, NativeEventKind::Input),
                (node_index, NativeEventKind::Change),
            ])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "content process checkable input/change events".into(),
                reason: "native checkable event batch was empty".into(),
            })?;
            let evaluation = runtime.evaluate_with_host_events(
                &event_batch,
                &next,
                document_url,
                document_origin,
                viewport,
            )?;
            apply_content_event_history(
                &evaluation.commands,
                document_url,
                document_origin,
                runtime,
                &mut history,
            )?;
            scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
            let (effects, _) = apply_document_commands_with_font_face_ack(
                &mut next,
                runtime,
                document_url,
                document_origin,
                viewport,
                &evaluation.commands,
                false,
            )?;
            events.extend(effects);
        }
        if let Some(form_id) = next.reset_control_form(node_id) {
            let command = serde_json::json!({
                "kind": "activateFormReset",
                "node_index": form_id.index(),
            });
            let command =
                serde_json::to_string(&command).map_err(|_| NativeEngineError::Worker {
                    operation: "activate native reset button".into(),
                    reason: "reset form target could not be encoded".into(),
                })?;
            let source = format!("globalThis.__glassApplyNativeCommand({command})");
            let reset =
                runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
            apply_content_event_history(
                &reset.commands,
                document_url,
                document_origin,
                runtime,
                &mut history,
            )?;
            scroll_commands.extend(extract_scroll_commands(&reset.commands));
            let (effects, _) = apply_document_commands_with_font_face_ack(
                &mut next,
                runtime,
                document_url,
                document_origin,
                viewport,
                &reset.commands,
                false,
            )?;
            events.extend(effects);
        } else if let Some(form_id) = next.submit_control_form(node_id) {
            let invalid = next.invalid_form_controls(form_id, Some(node_id))?;
            if invalid.is_empty() {
                if dispatch_submit_event(
                    &mut next,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    form_id,
                    Some(node_id),
                    &mut events,
                    &mut history,
                    &mut scroll_commands,
                )? {
                    let request = next.form_submission_request_with_submitter(
                        form_id,
                        document_url,
                        Some(node_id),
                    )?;
                    if form_action_allows(loader.as_deref_mut(), document_url, &request.url)? {
                        let object_url = runtime.object_url_resource(&request.url)?;
                        navigation = Some(NativeContentNavigation {
                            node_index: form_id.index(),
                            href: request.url,
                            submitter_node_index: Some(node_id.index()),
                            target: next.form_submission_target(form_id, Some(node_id))?,
                            method: request.method,
                            body: request.body,
                            body_content_type: request.body_content_type,
                            location: false,
                            replace_history: false,
                            object_url,
                        });
                    }
                }
            } else {
                dispatch_invalid_events(
                    &mut next,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    &invalid,
                    &mut events,
                    &mut history,
                    &mut scroll_commands,
                )?;
            }
        }
    } else {
        if let Some(checkable_pre_activation) = checkable_pre_activation {
            next.restore_checkable_pre_activation(checkable_pre_activation)?;
        }
        events.push((node_id, NativeEventKind::Click));
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process click event effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        navigate_to_sources: None,
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation,
        csp_violations: Vec::new(),
        allowed: click_allowed,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

pub(super) fn dispatch_submit_event(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    form_id: NativeNodeId,
    submitter: Option<NativeNodeId>,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
) -> Result<bool, NativeEngineError> {
    let event_batch = host_submit_event_batch(form_id.index(), submitter.map(NativeNodeId::index))?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process submit event".into(),
            reason: "native submit event batch was empty".into(),
        })?;
    let evaluation = runtime.evaluate_with_host_events(
        &event_batch,
        document,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
    let allowed = evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process submit event".into(),
            reason: "native submit event result was invalid".into(),
        })?;
    events.push((form_id, NativeEventKind::Submit));
    let (effects, _) = apply_document_commands_with_font_face_ack(
        document,
        runtime,
        document_url,
        document_origin,
        viewport,
        &evaluation.commands,
        false,
    )?;
    events.extend(effects);
    Ok(allowed)
}

pub(super) fn dispatch_invalid_events(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    invalid: &[NativeNodeId],
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let metadata = invalid
        .iter()
        .map(|id| (id.index(), NativeEventKind::Invalid))
        .collect::<Vec<_>>();
    let Some(event_batch) = host_event_batch(&metadata)? else {
        return Ok(());
    };
    let evaluation = runtime.evaluate_with_host_events(
        &event_batch,
        document,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
    events.extend(
        invalid
            .iter()
            .copied()
            .map(|id| (id, NativeEventKind::Invalid)),
    );
    let (effects, _) = apply_document_commands_with_font_face_ack(
        document,
        runtime,
        document_url,
        document_origin,
        viewport,
        &evaluation.commands,
        false,
    )?;
    events.extend(effects);
    Ok(())
}

pub(super) fn mutate_type_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    text: &str,
    loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = next.apply_type(node_id, text)?;
    let default_events = events.clone();
    let default_event_metadata = default_events
        .iter()
        .map(|(node_id, kind)| (node_id.index(), *kind))
        .collect::<Vec<_>>();
    for index in 0..default_events.len() {
        let event_batch = host_event_batch_at(&default_event_metadata, index)?;
        let Some(event_batch) = event_batch else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            document_url,
            document_origin,
            viewport,
        )?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        let (effects, _) = apply_document_commands_with_font_face_ack(
            &mut next,
            runtime,
            document_url,
            document_origin,
            viewport,
            &evaluation.commands,
            false,
        )?;
        events.extend(effects);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process type event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        navigate_to_sources: None,
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
        csp_violations: Vec::new(),
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

pub(super) enum NativeFormAction {
    Clear,
    Select(String),
    Hover,
    Drag { destination_node_index: u32 },
    Upload(Vec<NativeFile>),
}

pub(super) fn mutate_form_action_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    action: NativeFormAction,
    loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = match action {
        NativeFormAction::Clear => next.apply_clear(node_id)?,
        NativeFormAction::Select(value) => next.apply_select(node_id, &value)?,
        NativeFormAction::Hover => next.apply_hover(node_id)?,
        NativeFormAction::Drag {
            destination_node_index,
        } => next.apply_drag(
            node_id,
            NativeNodeId::from_parts(current.generation(), destination_node_index),
        )?,
        NativeFormAction::Upload(files) => next.apply_upload(node_id, &files)?,
    };
    let default_events = events.clone();
    let default_event_metadata = default_events
        .iter()
        .map(|(node_id, kind)| (node_id.index(), *kind))
        .collect::<Vec<_>>();
    for index in 0..default_events.len() {
        let event_batch = host_event_batch_at(&default_event_metadata, index)?;
        let Some(event_batch) = event_batch else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            document_url,
            document_origin,
            viewport,
        )?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        let (effects, _) = apply_document_commands_with_font_face_ack(
            &mut next,
            runtime,
            document_url,
            document_origin,
            viewport,
            &evaluation.commands,
            false,
        )?;
        events.extend(effects);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process form event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        navigate_to_sources: None,
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
        csp_violations: Vec::new(),
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

pub(super) fn mutate_key_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
    loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_edit_key(key)?;
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    if current.focused_text_control()? != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "key press target is not the focused text control".into(),
        });
    }
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = vec![(node_id, NativeEventKind::KeyDown)];
    let keydown_event_batch = host_key_event_batch(node_index, NativeEventKind::KeyDown, key)?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keydown event bridge".into(),
            reason: "native keydown event batch was empty".into(),
        })?;
    let keydown = runtime.evaluate_with_host_events(
        &keydown_event_batch,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keydown.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keydown.commands));
    let keydown_allowed = keydown
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keydown event bridge".into(),
            reason: "native keydown event result was invalid".into(),
        })?;
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &keydown.commands,
        false,
    )?;
    events.extend(effects);

    if keydown_allowed
        && next
            .focused_text_control()
            .is_ok_and(|focused| focused == node_id)
    {
        let input_events = next.apply_key_press(node_id, key)?;
        events.extend(input_events.clone());
        for (event_node, event_kind) in input_events {
            let event_batch = host_event_batch(&[(event_node.index(), event_kind)])?;
            let Some(event_batch) = event_batch else {
                continue;
            };
            let evaluation = runtime.evaluate_with_host_events(
                &event_batch,
                &next,
                document_url,
                document_origin,
                viewport,
            )?;
            apply_content_event_history(
                &evaluation.commands,
                document_url,
                document_origin,
                runtime,
                &mut history,
            )?;
            scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
            let (effects, _) = apply_document_commands_with_font_face_ack(
                &mut next,
                runtime,
                document_url,
                document_origin,
                viewport,
                &evaluation.commands,
                false,
            )?;
            events.extend(effects);
        }
    }

    events.push((node_id, NativeEventKind::KeyUp));
    let keyup_event_batch = host_key_event_batch(node_index, NativeEventKind::KeyUp, key)?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keyup event bridge".into(),
            reason: "native keyup event batch was empty".into(),
        })?;
    let keyup = runtime.evaluate_with_host_events(
        &keyup_event_batch,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keyup.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keyup.commands));
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &keyup.commands,
        false,
    )?;
    events.extend(effects);
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process key press effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        navigate_to_sources: None,
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
        csp_violations: Vec::new(),
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

pub(super) fn mutate_key_event_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
    kind: NativeEventKind,
    modifiers: i64,
    activate_space_on_keyup: bool,
    mut loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_key(key)?;
    if !matches!(kind, NativeEventKind::KeyDown | NativeEventKind::KeyUp) {
        return Err(NativeEngineError::invalid(
            "content-process key event",
            "kind must be keydown or keyup",
        ));
    }
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let focused_id = current.focused_node();
    if current.keyboard_event_target(focused_id) != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "key event target is not the focused page target".into(),
        });
    }
    let keyboard_link_target = current.has_native_keyboard_link_activation(focused_id);
    let keyboard_checkable_kind = current.keyboard_checkable_kind(focused_id);
    let radio_arrow_target =
        current.can_apply_radio_group_arrow_navigation(focused_id, key, modifiers);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let event_batch = host_key_event_batch_with_modifiers(node_index, kind, key, modifiers)?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process key event bridge".into(),
            reason: "native key event batch was empty".into(),
        })?;
    let evaluation = runtime.evaluate_with_host_events(
        &event_batch,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    let event_allowed = evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process key event".into(),
            reason: "native key event result was invalid".into(),
        })?;
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
    let mut events = vec![(node_id, kind)];
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &evaluation.commands,
        false,
    )?;
    events.extend(effects);
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader.as_deref_mut() {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    if kind == NativeEventKind::KeyDown
        && event_allowed
        && radio_arrow_target
        && next.focused_node() == focused_id
        && next.can_apply_radio_group_arrow_navigation(focused_id, key, modifiers)
    {
        let default_events = next.apply_key_default(focused_id, key, modifiers)?;
        events.extend(default_events.clone());
        let default_event_metadata = default_events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        for index in 0..default_events.len() {
            let Some(event_batch) = host_event_batch_at(&default_event_metadata, index)? else {
                continue;
            };
            let evaluation = runtime.evaluate_with_host_events(
                &event_batch,
                &next,
                document_url,
                document_origin,
                viewport,
            )?;
            apply_content_event_history(
                &evaluation.commands,
                document_url,
                document_origin,
                runtime,
                &mut history,
            )?;
            scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
            let (effects, _) = apply_document_commands_with_font_face_ack(
                &mut next,
                runtime,
                document_url,
                document_origin,
                viewport,
                &evaluation.commands,
                false,
            )?;
            events.extend(effects);
        }
    }
    let keyboard_button_target_remains = next.has_native_keyboard_button_activation(focused_id);
    let keyboard_link_target_remains = key == "Enter"
        && keyboard_link_target
        && next.has_native_keyboard_link_activation(focused_id);
    let keyboard_checkable_target_remains = key == " "
        && activate_space_on_keyup
        && keyboard_checkable_kind
            .is_some_and(|expected| next.keyboard_checkable_kind(focused_id) == Some(expected));
    let should_activate = next.focused_node() == focused_id
        && ((kind == NativeEventKind::KeyDown
            && key == "Enter"
            && event_allowed
            && (keyboard_button_target_remains || keyboard_link_target_remains))
            || (kind == NativeEventKind::KeyUp
                && key == " "
                && activate_space_on_keyup
                && (keyboard_button_target_remains || keyboard_checkable_target_remains)
                && (event_allowed || keyboard_checkable_target_remains)));
    if should_activate {
        let (clicked, click_mutation) = mutate_click_with_event_preflight(
            &next,
            runtime,
            document_url,
            document_origin,
            viewport,
            focused_id.index(),
            0,
            loader,
        )?;
        let mutation = prepend_native_key_effects(click_mutation, events, history, scroll_commands);
        if mutation.events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process keyboard activation effects",
                MAX_NATIVE_EFFECTS,
                mutation.events.len(),
            ));
        }
        return Ok((clicked, mutation));
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process key event effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            navigate_to_sources: None,
            events: events
                .into_iter()
                .map(|(node, kind)| NativeContentEvent {
                    node_index: node.index(),
                    kind,
                })
                .collect(),
            navigation: None,
            csp_violations: Vec::new(),
            allowed: event_allowed,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}

pub(super) fn prepend_native_key_effects(
    mut mutation: NativeContentMutation,
    events: Vec<(NativeNodeId, NativeEventKind)>,
    mut history: Vec<NativeScriptCommand>,
    mut scroll_commands: Vec<NativeScriptCommand>,
) -> NativeContentMutation {
    let mut events = events
        .into_iter()
        .map(|(node, kind)| NativeContentEvent {
            node_index: node.index(),
            kind,
        })
        .collect::<Vec<_>>();
    events.append(&mut mutation.events);
    mutation.events = events;
    history.append(&mut mutation.history);
    mutation.history = history;
    scroll_commands.append(&mut mutation.scroll_commands);
    mutation.scroll_commands = scroll_commands;
    mutation
}

pub(super) fn append_native_key_effects(
    mut mutation: NativeContentMutation,
    mut events: Vec<(NativeNodeId, NativeEventKind)>,
    mut history: Vec<NativeScriptCommand>,
    mut scroll_commands: Vec<NativeScriptCommand>,
) -> NativeContentMutation {
    mutation
        .events
        .extend(events.drain(..).map(|(node, kind)| NativeContentEvent {
            node_index: node.index(),
            kind,
        }));
    mutation.history.append(&mut history);
    mutation.scroll_commands.append(&mut scroll_commands);
    mutation
}

pub(super) fn mutate_key_shortcut_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
    modifiers: i64,
    apply_default: bool,
    mut loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_key(key)?;
    if !(0..=15).contains(&modifiers) {
        return Err(NativeEngineError::invalid(
            "content-process shortcut modifiers",
            "must be an integer mask from 0 through 15",
        ));
    }
    if apply_default && !should_apply_native_key_default(key, modifiers) {
        return Err(NativeEngineError::invalid(
            "content-process shortcut default",
            "default behavior is only valid for Ctrl/Meta+A, bounded text editing, radio-group arrow navigation, or Tab focus traversal",
        ));
    }
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let focused_id = current.focused_node();
    if current.keyboard_event_target(focused_id) != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "shortcut target is not the focused page target".into(),
        });
    }
    let radio_arrow_target =
        current.can_apply_radio_group_arrow_navigation(focused_id, key, modifiers);
    let text_default_target = current
        .focused_text_control()
        .is_ok_and(|focused| focused == focused_id);
    let keyboard_button_target = current.has_native_keyboard_button_activation(focused_id);
    let keyboard_checkable_kind = current.keyboard_checkable_kind(focused_id);
    let keyboard_link_target =
        key == "Enter" && current.has_native_keyboard_link_activation(focused_id);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut activation_mutation = None;
    let keydown_event_batch =
        host_key_event_batch_with_modifiers(node_index, NativeEventKind::KeyDown, key, modifiers)?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "content process shortcut event bridge".into(),
                reason: "native shortcut keydown batch was empty".into(),
            })?;
    let keydown = runtime.evaluate_with_host_events(
        &keydown_event_batch,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keydown.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keydown.commands));
    let keydown_allowed = keydown
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process shortcut keydown".into(),
            reason: "native shortcut keydown result was invalid".into(),
        })?;
    let mut events = vec![(node_id, NativeEventKind::KeyDown)];
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &keydown.commands,
        false,
    )?;
    events.extend(effects);
    if keydown_allowed && apply_default && next.focused_node() == focused_id {
        let default_events = if key == "Tab" {
            next.apply_tab_focus(modifiers & 8 != 0)?
        } else if radio_arrow_target {
            if next.can_apply_radio_group_arrow_navigation(focused_id, key, modifiers) {
                next.apply_key_default(focused_id, key, modifiers)?
            } else {
                Vec::new()
            }
        } else if text_default_target
            && next
                .focused_text_control()
                .is_ok_and(|focused| focused == focused_id)
        {
            next.apply_key_default(focused_id, key, modifiers)?
        } else {
            Vec::new()
        };
        events.extend(default_events.clone());
        let default_event_metadata = default_events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        if let Some(event_batch) = host_event_batch(&default_event_metadata)? {
            let mut event_index = 0;
            while event_index < event_batch.len() {
                let is_focus_transition = |index: usize| {
                    matches!(
                        default_events[index].1,
                        NativeEventKind::Blur
                            | NativeEventKind::FocusOut
                            | NativeEventKind::Focus
                            | NativeEventKind::FocusIn
                    )
                };
                let batch_end = if is_focus_transition(event_index) {
                    (event_index + 1..event_batch.len())
                        .find(|index| !is_focus_transition(*index))
                        .unwrap_or(event_batch.len())
                } else {
                    event_index + 1
                };
                let evaluation = runtime.evaluate_with_host_events(
                    &event_batch[event_index..batch_end],
                    &next,
                    document_url,
                    document_origin,
                    viewport,
                )?;
                apply_content_event_history(
                    &evaluation.commands,
                    document_url,
                    document_origin,
                    runtime,
                    &mut history,
                )?;
                scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
                let (effects, _) = apply_document_commands_with_font_face_ack(
                    &mut next,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    &evaluation.commands,
                    false,
                )?;
                events.extend(effects);
                event_index = batch_end;
            }
        }
    }
    if keydown_allowed
        && key == "Enter"
        && next.focused_node() == focused_id
        && ((keyboard_button_target && next.has_native_keyboard_button_activation(focused_id))
            || (keyboard_link_target && next.has_native_keyboard_link_activation(focused_id)))
    {
        dispatch_scroll_events(
            &mut next,
            runtime,
            document_url,
            document_origin,
            viewport,
            &mut scroll_commands,
            &mut events,
            &mut history,
        )?;
        let (clicked, click_mutation) = mutate_click_with_event_preflight(
            &next,
            runtime,
            document_url,
            document_origin,
            viewport,
            focused_id.index(),
            0,
            loader.as_deref_mut(),
        )?;
        activation_mutation = Some(prepend_native_key_effects(
            click_mutation,
            std::mem::take(&mut events),
            std::mem::take(&mut history),
            std::mem::take(&mut scroll_commands),
        ));
        next = clicked;
    }
    let keyup_node_id = next.keyboard_event_target(next.focused_node());
    let keyup_event_batch = host_key_event_batch_with_modifiers(
        keyup_node_id.index(),
        NativeEventKind::KeyUp,
        key,
        modifiers,
    )?
    .ok_or_else(|| NativeEngineError::Worker {
        operation: "content process shortcut event bridge".into(),
        reason: "native shortcut keyup batch was empty".into(),
    })?;
    let keyup = runtime.evaluate_with_host_events(
        &keyup_event_batch,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keyup.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keyup.commands));
    events.push((keyup_node_id, NativeEventKind::KeyUp));
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &keyup.commands,
        false,
    )?;
    events.extend(effects);
    let keyboard_checkable_target_remains = modifiers == 0
        && keyboard_checkable_kind
            .is_some_and(|expected| next.keyboard_checkable_kind(focused_id) == Some(expected));
    if keydown_allowed
        && key == " "
        && next.focused_node() == focused_id
        && ((keyboard_button_target && next.has_native_keyboard_button_activation(focused_id))
            || keyboard_checkable_target_remains)
    {
        dispatch_scroll_events(
            &mut next,
            runtime,
            document_url,
            document_origin,
            viewport,
            &mut scroll_commands,
            &mut events,
            &mut history,
        )?;
        let (clicked, click_mutation) = mutate_click_with_event_preflight(
            &next,
            runtime,
            document_url,
            document_origin,
            viewport,
            focused_id.index(),
            0,
            loader.as_deref_mut(),
        )?;
        activation_mutation = Some(prepend_native_key_effects(
            click_mutation,
            std::mem::take(&mut events),
            std::mem::take(&mut history),
            std::mem::take(&mut scroll_commands),
        ));
        next = clicked;
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process shortcut effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    if let Some(activation_mutation) = activation_mutation {
        let mut mutation =
            append_native_key_effects(activation_mutation, events, history, scroll_commands);
        mutation.document = next.to_content_wire();
        if mutation.events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process shortcut activation effects",
                MAX_NATIVE_EFFECTS,
                mutation.events.len(),
            ));
        }
        return Ok((next, mutation));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            navigate_to_sources: None,
            events: events
                .into_iter()
                .map(|(node, kind)| NativeContentEvent {
                    node_index: node.index(),
                    kind,
                })
                .collect(),
            navigation: None,
            csp_violations: Vec::new(),
            allowed: true,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}
