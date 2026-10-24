use super::*;

pub(super) fn should_apply_native_key_default(key: &str, modifiers: i64) -> bool {
    let primary_modifier = modifiers & (2 | 4) != 0;
    if primary_modifier {
        return key.eq_ignore_ascii_case("a") && modifiers & 1 == 0;
    }
    modifiers & 1 == 0
        && (key.chars().count() == 1
            || matches!(
                key,
                "Backspace"
                    | "Delete"
                    | "ArrowLeft"
                    | "ArrowRight"
                    | "ArrowUp"
                    | "ArrowDown"
                    | "Home"
                    | "End"
                    | "Tab"
            ))
}

pub(super) fn split_location_navigation(
    runtime: &NativeJavaScriptRuntime,
    commands: Vec<NativeScriptCommand>,
) -> Result<(Vec<NativeScriptCommand>, Option<NativeContentNavigation>), NativeEngineError> {
    let mut retained = Vec::new();
    let mut navigation = None;
    for command in commands {
        match command {
            NativeScriptCommand::Navigate { href, replace } => {
                validate_url_text("script location href", &href)?;
                if navigation.is_some() {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "one lifecycle event cannot activate multiple navigations".into(),
                    });
                }
                let object_url = runtime.object_url_resource(&href)?;
                navigation = Some(NativeContentNavigation {
                    node_index: 0,
                    href,
                    submitter_node_index: None,
                    target: "_self".into(),
                    method: NativeNavigationMethod::Get,
                    body: None,
                    body_content_type: None,
                    location: true,
                    replace_history: replace,
                    object_url,
                });
            }
            command => retained.push(command),
        }
    }
    Ok((retained, navigation))
}

pub(super) fn extract_history_commands(
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

pub(super) fn extract_scroll_commands(
    commands: &[NativeScriptCommand],
) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| matches!(command, NativeScriptCommand::ScrollTo { .. }))
        .cloned()
        .collect()
}

/// Deliver scroll events for commands emitted by the page realm and apply any
/// synchronous mutations produced by those listeners. The content process is
/// the owner of page JavaScript, so scroll listeners must run here before the
/// resulting document snapshot crosses back to the parent coordinator.
pub(super) fn dispatch_scroll_events(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    pending_commands: &mut Vec<NativeScriptCommand>,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let mut pending = std::mem::take(pending_commands);
    let mut cursor = 0;
    while cursor < pending.len() {
        let NativeScriptCommand::ScrollTo { node_index, .. } = pending[cursor] else {
            cursor += 1;
            continue;
        };
        let event_node_index = if node_index != 0 {
            node_index
        } else {
            u32::MAX
        };
        let event_batch = host_event_batch(&[(event_node_index, NativeEventKind::Scroll)])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "content process scroll event".into(),
                reason: "native scroll event batch was empty".into(),
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
        let new_scroll_commands = extract_scroll_commands(&evaluation.commands);
        if pending.len().saturating_add(new_scroll_commands.len()) > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process scroll event commands",
                MAX_NATIVE_EFFECTS,
                pending.len().saturating_add(new_scroll_commands.len()),
            ));
        }
        events.push((
            NativeNodeId::from_parts(document.generation(), event_node_index),
            NativeEventKind::Scroll,
        ));
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
        pending.extend(new_scroll_commands);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process scroll event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        cursor += 1;
    }
    *pending_commands = pending;
    Ok(())
}

pub(super) fn apply_content_event_history(
    commands: &[NativeScriptCommand],
    document_url: &mut String,
    document_origin: &NativeOrigin,
    runtime: &NativeJavaScriptRuntime,
    history: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let commands = extract_history_commands(commands);
    if commands.is_empty() {
        return Ok(());
    }
    let mut url = Some(document_url.clone());
    apply_content_runtime_history(&commands, &mut url, document_origin, runtime)?;
    *document_url = url.ok_or_else(|| NativeEngineError::Worker {
        operation: "content process history".into(),
        reason: "history did not preserve the committed document URL".into(),
    })?;
    history.extend(commands);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_content_resource_event_evaluation(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    evaluation: NativeScriptEvaluation,
    history: &mut Vec<NativeScriptCommand>,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
    script_result: &mut NativePageScriptResult,
    navigation: &mut Option<ScriptNavigationTarget>,
) -> Result<(), NativeEngineError> {
    let previous_base_url = document.effective_base_url(document_url);
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        history,
    )?;
    let effects = apply_page_script_evaluation(
        document,
        runtime,
        NativePageScriptContext {
            document_url,
            origin: document_origin,
            viewport,
        },
        evaluation,
        NativePageScriptCommandQueues {
            pending_fetches: &mut script_result.pending_fetches,
            websocket_commands: &mut script_result.websocket_commands,
            event_source_commands: &mut script_result.event_source_commands,
        },
        scroll_commands,
        &mut script_result.navigation,
    )?;
    events.extend(effects.into_iter().map(|(node_index, kind)| {
        (
            NativeNodeId::from_parts(document.generation(), node_index),
            kind,
        )
    }));
    if document.effective_base_url(document_url) != previous_base_url {
        document.clear_video_poster_loads();
    }
    if let Some(page_navigation) = script_result.navigation.take() {
        *navigation = Some(ScriptNavigationTarget::Location {
            href: page_navigation.href,
            replace_history: page_navigation.replace_history,
        });
    }
    Ok(())
}

pub(super) fn resolve_content_history_document_url(
    commands: &[NativeScriptCommand],
    base_url: &str,
    document_origin: &NativeOrigin,
) -> Result<String, NativeEngineError> {
    let mut url = base_url.to_owned();
    for command in commands {
        match command {
            NativeScriptCommand::HistoryPushState { href, state }
            | NativeScriptCommand::HistoryReplaceState { href, state } => {
                validate_content_history_state(state)?;
                url = resolve_content_history_href(&url, document_origin, href)?;
            }
            NativeScriptCommand::HistoryGo { .. } => {}
            _ => {
                return Err(NativeEngineError::Worker {
                    operation: "content process history".into(),
                    reason: "non-history command reached history URL resolution".into(),
                });
            }
        }
    }
    Ok(url)
}

pub(super) fn apply_content_runtime_history(
    commands: &[NativeScriptCommand],
    document_url: &mut Option<String>,
    document_origin: &NativeOrigin,
    runtime: &NativeJavaScriptRuntime,
) -> Result<(), NativeEngineError> {
    let Some(current_url) = document_url.as_deref() else {
        return Err(NativeEngineError::Worker {
            operation: "content process history".into(),
            reason: "history requires a committed document URL".into(),
        });
    };
    let mut base_url = current_url.to_owned();
    let mut traversal_seen = false;
    for command in commands {
        match command {
            NativeScriptCommand::HistoryPushState { href, state } => {
                if traversal_seen {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason:
                            "history state mutation cannot follow traversal in one script batch"
                                .into(),
                    });
                }
                let url = resolve_content_history_href(&base_url, document_origin, href)?;
                validate_content_history_state(state)?;
                let length = runtime.history_length().saturating_add(1);
                runtime.set_history_state(state.clone());
                runtime.set_history_length(length);
                *document_url = Some(url.clone());
                base_url = url;
            }
            NativeScriptCommand::HistoryReplaceState { href, state } => {
                if traversal_seen {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason:
                            "history state mutation cannot follow traversal in one script batch"
                                .into(),
                    });
                }
                let url = resolve_content_history_href(&base_url, document_origin, href)?;
                validate_content_history_state(state)?;
                runtime.set_history_state(state.clone());
                *document_url = Some(url.clone());
                base_url = url;
            }
            NativeScriptCommand::HistoryGo { delta } => {
                if traversal_seen {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "a script batch cannot request multiple history traversals".into(),
                    });
                }
                if delta.unsigned_abs() > 1024 {
                    return Err(NativeEngineError::limit(
                        "content-process history traversal delta",
                        1024,
                        delta.unsigned_abs() as usize,
                    ));
                }
                traversal_seen = true;
            }
            _ => unreachable!("history extraction only returns history commands"),
        }
    }
    Ok(())
}

pub(super) fn resolve_content_history_href(
    base_url: &str,
    document_origin: &NativeOrigin,
    href: &str,
) -> Result<String, NativeEngineError> {
    validate_url_text("content-process history URL", href)?;
    let base = Url::parse(base_url).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "content-process history base URL is malformed".into(),
    })?;
    let target = if let Ok(absolute) = Url::parse(href) {
        absolute
    } else {
        base.join(href)
            .map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "content-process history URL is malformed".into(),
            })?
    };
    if NativeOrigin::from_url(&target)? != *document_origin {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "content-process history URL must be same-origin".into(),
        });
    }
    let target = target.to_string();
    validate_url_text("content-process history URL", &target)?;
    Ok(target)
}

pub(super) fn validate_content_history_state(state: &Value) -> Result<(), NativeEngineError> {
    let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
        operation: "serialize content-process history state".into(),
        reason: "history state could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process history state",
            MAX_NATIVE_HISTORY_STATE_BYTES,
            encoded.len(),
        ));
    }
    Ok(())
}

pub(super) fn mutate_before_unload(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let event_batch =
        host_event_batch(&[(u32::MAX, NativeEventKind::BeforeUnload)])?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process beforeunload".into(),
                reason: "beforeunload event batch was empty".into(),
            }
        })?;
    let evaluation = runtime.evaluate_with_host_events(
        &event_batch,
        current,
        document_url,
        document_origin,
        viewport,
    )?;
    let allowed = evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process beforeunload".into(),
            reason: "beforeunload event result was invalid".into(),
        })?;
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut events = vec![(
        NativeNodeId::from_parts(current.generation(), u32::MAX),
        NativeEventKind::BeforeUnload,
    )];
    let mut scroll_commands = extract_scroll_commands(&evaluation.commands);
    let (commands, navigation) = split_location_navigation(runtime, evaluation.commands)?;
    apply_content_event_history(
        &commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &commands,
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
            "native beforeunload effects",
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
            navigation,
            csp_violations: Vec::new(),
            allowed,
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

pub(super) fn mutate_lifecycle_events(
    request_id: u64,
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    kinds: &[NativeEventKind],
    loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    if kinds.is_empty() {
        return Ok((
            current.clone(),
            NativeContentMutation {
                document: current.to_content_wire(),
                navigate_to_sources: None,
                events: Vec::new(),
                navigation: None,
                csp_violations: Vec::new(),
                allowed: true,
                history: Vec::new(),
                scroll_commands: Vec::new(),
                storage_events: Vec::new(),
                indexed_db_changes: Vec::new(),
                dialogs: Vec::new(),
                popups: Vec::new(),
                post_messages: Vec::new(),
                window_closes: Vec::new(),
                window_navigations: Vec::new(),
                window_name: String::new(),
            },
        ));
    }
    let metadata = kinds
        .iter()
        .map(|kind| (u32::MAX, *kind))
        .collect::<Vec<_>>();
    let event_batch = host_event_batch(&metadata)?.ok_or_else(|| NativeEngineError::Worker {
        operation: "content process lifecycle events".into(),
        reason: "lifecycle event batch was empty".into(),
    })?;
    let event_types = event_batch
        .iter()
        .map(|event| event.event_type.clone())
        .collect::<Vec<_>>();
    let listener_stage = ContentLoadStage::begin(request_id, "lifecycle_listener_lookup");
    let has_listeners = event_types
        .iter()
        .any(|event_type| current.has_inline_event_handler(event_type))
        || runtime.has_host_event_listeners(&event_types)? != Some(false);
    listener_stage.finish(Some(event_types.len()));
    if !has_listeners {
        return Ok((
            current.clone(),
            NativeContentMutation {
                document: current.to_content_wire(),
                navigate_to_sources: None,
                events: kinds
                    .iter()
                    .copied()
                    .map(|kind| NativeContentEvent {
                        node_index: u32::MAX,
                        kind,
                    })
                    .collect(),
                navigation: None,
                csp_violations: Vec::new(),
                allowed: true,
                history: Vec::new(),
                scroll_commands: Vec::new(),
                storage_events: Vec::new(),
                indexed_db_changes: Vec::new(),
                dialogs: Vec::new(),
                popups: Vec::new(),
                post_messages: Vec::new(),
                window_closes: Vec::new(),
                window_navigations: Vec::new(),
                window_name: String::new(),
            },
        ));
    }
    let evaluation_stage = ContentLoadStage::begin(request_id, "lifecycle_event_evaluation");
    let evaluation = runtime.evaluate_with_host_events(
        &event_batch,
        current,
        document_url,
        document_origin,
        viewport,
    )?;
    evaluation_stage.finish(None);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = extract_scroll_commands(&evaluation.commands);
    let (commands, navigation) = split_location_navigation(runtime, evaluation.commands)?;
    apply_content_event_history(
        &commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    let command_stage = ContentLoadStage::begin(request_id, "lifecycle_document_commands");
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &commands,
        false,
    )?;
    command_stage.finish(Some(commands.len()));
    let mut events = kinds
        .iter()
        .copied()
        .map(|kind| NativeContentEvent {
            node_index: u32::MAX,
            kind,
        })
        .collect::<Vec<_>>();
    events.extend(effects.into_iter().map(|(node, kind)| NativeContentEvent {
        node_index: node.index(),
        kind,
    }));
    let mut event_effects = events
        .iter()
        .map(|event| {
            (
                NativeNodeId::from_parts(current.generation(), event.node_index),
                event.kind,
            )
        })
        .collect::<Vec<_>>();
    let scroll_stage = ContentLoadStage::begin(request_id, "lifecycle_scroll_events");
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut event_effects,
        &mut history,
    )?;
    scroll_stage.finish(Some(scroll_commands.len()));
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, document_url)?;
        refresh_inline_style_policy(&mut next, loader, document_url)?;
    }
    events = event_effects
        .into_iter()
        .map(|(node, kind)| NativeContentEvent {
            node_index: node.index(),
            kind,
        })
        .collect();
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native lifecycle effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let serialization_stage =
        ContentLoadStage::begin(request_id, "lifecycle_document_serialization");
    let document = next.to_content_wire();
    serialization_stage.finish(Some(document.nodes.len()));
    Ok((
        next.clone(),
        NativeContentMutation {
            document,
            navigate_to_sources: None,
            events,
            navigation,
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

pub(super) fn mutate_hash_change(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    old_url: &str,
    new_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    loader: Option<&mut NativeResourceLoader>,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let page_events = NativePageEventBatch {
        hash_change_events: vec![NativeHashChangeEvent {
            old_url: old_url.to_owned(),
            new_url: new_url.to_owned(),
        }],
        ..NativePageEventBatch::default()
    };
    let evaluation = runtime.evaluate_with_page_events(
        "undefined;",
        current,
        new_url,
        document_origin,
        viewport,
        &page_events,
    )?;
    let mut next = current.clone();
    next.set_css_target_from_url(new_url)?;
    let mut history = Vec::new();
    let mut scroll_commands = extract_scroll_commands(&evaluation.commands);
    let (commands, navigation) = split_location_navigation(runtime, evaluation.commands)?;
    apply_content_event_history(&commands, new_url, document_origin, runtime, &mut history)?;
    let mut events = vec![NativeContentEvent {
        node_index: u32::MAX,
        kind: NativeEventKind::HashChange,
    }];
    let (effects, _) = apply_document_commands_with_font_face_ack(
        &mut next,
        runtime,
        new_url.as_str(),
        document_origin,
        viewport,
        &commands,
        false,
    )?;
    events.extend(effects.into_iter().map(|(node, kind)| NativeContentEvent {
        node_index: node.index(),
        kind,
    }));
    let mut event_effects = events
        .iter()
        .map(|event| {
            (
                NativeNodeId::from_parts(current.generation(), event.node_index),
                event.kind,
            )
        })
        .collect::<Vec<_>>();
    dispatch_scroll_events(
        &mut next,
        runtime,
        new_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut event_effects,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, new_url)?;
        refresh_inline_style_policy(&mut next, loader, new_url)?;
    }
    events = event_effects
        .into_iter()
        .map(|(node, kind)| NativeContentEvent {
            node_index: node.index(),
            kind,
        })
        .collect();
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native hashchange effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            navigate_to_sources: None,
            events,
            navigation,
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

pub(super) async fn mutate_script_document(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    commands: &[NativeScriptCommand],
    mut loader: Option<&mut NativeResourceLoader>,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<
    (
        NativeDocument,
        NativeContentMutation,
        NativePageScriptResult,
    ),
    NativeEngineError,
> {
    let previous_base_url = current.effective_base_url(document_url);
    let mut document_url = document_url.to_owned();
    let mut history = Vec::new();
    let mut effective_commands = commands.to_vec();
    let mut scroll_commands = extract_scroll_commands(&effective_commands);
    let mut dialogs = Vec::new();
    let mut dynamic_navigation = None;
    let mut dynamic_result = NativePageScriptResult::default();
    let mut next = current.clone();
    let (initial_events, font_face_follow_up_commands) =
        apply_document_commands_with_font_face_ack(
            &mut next,
            runtime,
            &document_url,
            document_origin,
            viewport,
            commands,
            true,
        )?;
    effective_commands.extend(font_face_follow_up_commands.iter().cloned());
    scroll_commands.extend(extract_scroll_commands(&font_face_follow_up_commands));
    let mut events = initial_events;
    if let Some(loader) = loader.as_deref_mut() {
        apply_pending_meta_content_security_policies(&mut next, loader, &document_url)?;
        refresh_inline_style_policy(&mut next, loader, &document_url)?;
        refresh_parent_broker_meta_csp(loader, parent_fetch_broker.as_deref_mut())?;
        dispatch_pending_csp_violations(
            &mut next,
            runtime,
            &mut document_url,
            document_origin,
            viewport,
            loader,
            &mut dynamic_result,
            &mut events,
            &mut history,
            &mut scroll_commands,
        )?;
    }
    next.refresh_image_loads(viewport);
    next.refresh_background_image_sources();
    let dynamic_sources = next.take_newly_attached_page_script_sources(
        &effective_commands,
        super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
        MAX_NATIVE_SCRIPT_BYTES,
    );
    if !dynamic_sources.is_empty() {
        dynamic_result = if let Some(loader) = loader.as_deref_mut() {
            execute_dynamic_page_scripts_with_loader(
                &mut next,
                runtime,
                dynamic_sources,
                loader,
                &document_url,
                document_origin,
                viewport,
                parent_fetch_broker.as_deref_mut(),
            )
            .await?
        } else {
            if dynamic_sources.iter().any(|source| {
                matches!(
                    source,
                    NativePageScriptSource::External { .. }
                        | NativePageScriptSource::ModuleExternal { .. }
                )
            }) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason:
                        "dynamic external/module scripts require a process-backed HTTP(S) document"
                            .into(),
                });
            }
            execute_dynamic_page_scripts_without_loader(
                &mut next,
                runtime,
                dynamic_sources,
                &document_url,
                document_origin,
                viewport,
            )?
        };
        if !dynamic_result.pending_script_sources.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: "dynamic page script".into(),
                reason: "dynamic external/module script loader handoff was unavailable".into(),
            });
        }
        events.extend(dynamic_result.events.drain(..).map(|(node_index, kind)| {
            (
                NativeNodeId::from_parts(next.generation(), node_index),
                kind,
            )
        }));
        scroll_commands.extend(std::mem::take(&mut dynamic_result.scroll_commands));
        dialogs.extend(std::mem::take(&mut dynamic_result.dialogs));
        if let Some(page_navigation) = dynamic_result.navigation.take() {
            dynamic_navigation = Some(ScriptNavigationTarget::Location {
                href: page_navigation.href,
                replace_history: page_navigation.replace_history,
            });
        }
        if let Some(loader) = loader.as_deref_mut() {
            apply_pending_meta_content_security_policies(&mut next, loader, &document_url)?;
            refresh_inline_style_policy(&mut next, loader, &document_url)?;
            refresh_parent_broker_meta_csp(loader, parent_fetch_broker.as_deref_mut())?;
            dispatch_pending_csp_violations(
                &mut next,
                runtime,
                &mut document_url,
                document_origin,
                viewport,
                loader,
                &mut dynamic_result,
                &mut events,
                &mut history,
                &mut scroll_commands,
            )?;
        }
    }
    retain_font_face_network_commands(&mut dynamic_result, font_face_follow_up_commands);
    let stylesheet_events = if let Some(loader) = loader.as_deref_mut() {
        load_dynamic_external_stylesheets(
            &mut next,
            runtime,
            loader,
            &document_url,
            viewport,
            parent_fetch_broker.as_deref_mut(),
        )
        .await?
    } else {
        Vec::new()
    };
    for (node_index, event_kind) in stylesheet_events {
        let Some(event_batch) = host_event_batch(&[(node_index, event_kind)])? else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            &document_url,
            document_origin,
            viewport,
        )?;
        apply_content_resource_event_evaluation(
            &mut next,
            runtime,
            &mut document_url,
            document_origin,
            viewport,
            evaluation,
            &mut history,
            &mut events,
            &mut scroll_commands,
            &mut dynamic_result,
            &mut dynamic_navigation,
        )?;
        events.push((
            NativeNodeId::from_parts(next.generation(), node_index),
            event_kind,
        ));
    }
    let preload_events = if let Some(loader) = loader.as_deref_mut() {
        load_external_image_preloads(
            &mut next,
            loader,
            &document_url,
            parent_fetch_broker.as_deref_mut(),
            None,
        )
        .await?
    } else {
        Vec::new()
    };
    for (node_index, event_kind) in preload_events {
        let Some(event_batch) = host_event_batch(&[(node_index, event_kind)])? else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            &document_url,
            document_origin,
            viewport,
        )?;
        apply_content_resource_event_evaluation(
            &mut next,
            runtime,
            &mut document_url,
            document_origin,
            viewport,
            evaluation,
            &mut history,
            &mut events,
            &mut scroll_commands,
            &mut dynamic_result,
            &mut dynamic_navigation,
        )?;
        events.push((
            NativeNodeId::from_parts(next.generation(), node_index),
            event_kind,
        ));
    }
    let validation_ids = events
        .iter()
        .filter(|(_, kind)| *kind == NativeEventKind::Invalid)
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    if !validation_ids.is_empty()
        && let Some(event_batch) = host_event_batch(
            &validation_ids
                .iter()
                .map(|id| (id.index(), NativeEventKind::Invalid))
                .collect::<Vec<_>>(),
        )?
    {
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            &document_url,
            document_origin,
            viewport,
        )?;
        apply_content_event_history(
            &evaluation.commands,
            &mut document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        let (effects, follow_up_commands) = apply_document_commands_with_font_face_ack(
            &mut next,
            runtime,
            &document_url,
            document_origin,
            viewport,
            &evaluation.commands,
            true,
        )?;
        events.extend(effects);
        retain_font_face_follow_up_commands(
            &mut effective_commands,
            &mut scroll_commands,
            &mut dynamic_result,
            follow_up_commands,
        );
    }
    let image_events = if let Some(loader) = loader.as_deref_mut() {
        load_external_images(
            &mut next,
            Some(runtime),
            loader,
            &document_url,
            viewport,
            parent_fetch_broker.as_deref_mut(),
            None,
        )
        .await?
    } else {
        Vec::new()
    };
    for (node_index, event_kind) in image_events {
        let Some(event_batch) = host_event_batch(&[(node_index, event_kind)])? else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            &document_url,
            document_origin,
            viewport,
        )?;
        apply_content_resource_event_evaluation(
            &mut next,
            runtime,
            &mut document_url,
            document_origin,
            viewport,
            evaluation,
            &mut history,
            &mut events,
            &mut scroll_commands,
            &mut dynamic_result,
            &mut dynamic_navigation,
        )?;
        events.push((
            NativeNodeId::from_parts(next.generation(), node_index),
            event_kind,
        ));
    }
    next.refresh_image_loads(viewport);
    if next.effective_base_url(&document_url) != previous_base_url {
        next.clear_video_poster_loads();
    }
    next.refresh_video_poster_loads();
    if let Some(loader) = loader.as_deref_mut() {
        load_external_video_posters(
            &mut next,
            loader,
            &document_url,
            parent_fetch_broker.as_deref_mut(),
            None,
        )
        .await?;
    }
    let media_events = if let Some(loader) = loader.as_deref_mut() {
        load_external_media(
            &mut next,
            Some(runtime),
            loader,
            &document_url,
            parent_fetch_broker.as_deref_mut(),
            None,
        )
        .await?
    } else {
        Vec::new()
    };
    for (node_index, event_kind) in media_events {
        let Some(event_batch) = host_event_batch(&[(node_index, event_kind)])? else {
            continue;
        };
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            &next,
            &document_url,
            document_origin,
            viewport,
        )?;
        apply_content_resource_event_evaluation(
            &mut next,
            runtime,
            &mut document_url,
            document_origin,
            viewport,
            evaluation,
            &mut history,
            &mut events,
            &mut scroll_commands,
            &mut dynamic_result,
            &mut dynamic_navigation,
        )?;
        events.push((
            NativeNodeId::from_parts(next.generation(), node_index),
            event_kind,
        ));
    }
    next.refresh_media_loads();
    if let Some(loader) = loader.as_deref_mut() {
        apply_pending_meta_content_security_policies(&mut next, loader, &document_url)?;
        refresh_inline_style_policy(&mut next, loader, &document_url)?;
        refresh_parent_broker_meta_csp(loader, parent_fetch_broker)?;
        dispatch_pending_csp_violations(
            &mut next,
            runtime,
            &mut document_url,
            document_origin,
            viewport,
            loader,
            &mut dynamic_result,
            &mut events,
            &mut history,
            &mut scroll_commands,
        )?;
    }
    next.refresh_background_image_sources();
    let mut navigation = script_navigation_target(&next, &document_url, &effective_commands)?;
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
        let invalid = next.invalid_form_controls(*form_id, *submitter)?;
        if invalid.is_empty() {
            if !dispatch_submit_event(
                &mut next,
                runtime,
                &mut document_url,
                document_origin,
                viewport,
                *form_id,
                *submitter,
                &mut events,
                &mut history,
                &mut scroll_commands,
            )? {
                navigation = None;
            }
        } else {
            dispatch_invalid_events(
                &mut next,
                runtime,
                &mut document_url,
                document_origin,
                viewport,
                &invalid,
                &mut events,
                &mut history,
                &mut scroll_commands,
            )?;
            navigation = None;
        }
    }
    if let Some(ScriptNavigationTarget::Form {
        form_id, submitter, ..
    }) = navigation.as_ref()
    {
        let request =
            next.form_submission_request_with_submitter(*form_id, &document_url, *submitter)?;
        if !form_action_allows(loader.as_deref_mut(), &document_url, &request.url)? {
            navigation = None;
        }
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        &mut document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if let Some(loader) = loader {
        apply_pending_meta_content_security_policies(&mut next, loader, &document_url)?;
        refresh_inline_style_policy(&mut next, loader, &document_url)?;
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process script mutation effects",
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
        navigation: navigation
            .map(|navigation| match navigation {
                ScriptNavigationTarget::Link { node_index, href } => Ok(NativeContentNavigation {
                    node_index,
                    object_url: runtime.object_url_resource(&href)?,
                    href,
                    submitter_node_index: None,
                    target: "_self".into(),
                    method: NativeNavigationMethod::Get,
                    body: None,
                    body_content_type: None,
                    location: false,
                    replace_history: false,
                }),
                ScriptNavigationTarget::Form {
                    form_id,
                    node_index,
                    submitter,
                    target,
                    ..
                } => {
                    let request = next.form_submission_request_with_submitter(
                        form_id,
                        &document_url,
                        submitter,
                    )?;
                    Ok(NativeContentNavigation {
                        node_index,
                        object_url: runtime.object_url_resource(&request.url)?,
                        href: request.url,
                        submitter_node_index: submitter.map(NativeNodeId::index),
                        target,
                        method: request.method,
                        body: request.body,
                        body_content_type: request.body_content_type,
                        location: false,
                        replace_history: false,
                    })
                }
                ScriptNavigationTarget::Location {
                    href,
                    replace_history,
                } => Ok(NativeContentNavigation {
                    node_index: 0,
                    object_url: runtime.object_url_resource(&href)?,
                    href,
                    submitter_node_index: None,
                    target: "_self".into(),
                    method: NativeNavigationMethod::Get,
                    body: None,
                    body_content_type: None,
                    location: true,
                    replace_history,
                }),
            })
            .transpose()?,
        csp_violations: Vec::new(),
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        dialogs,
        window_name: String::new(),
    };
    Ok((next, mutation, dynamic_result))
}
