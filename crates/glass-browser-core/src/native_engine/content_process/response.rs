use super::*;

pub(super) fn content_navigation_json(navigation: &NativeContentNavigation) -> Value {
    json!({
        "node_index": navigation.node_index,
        "href": navigation.href,
        "submitter_node_index": navigation.submitter_node_index,
        "target": navigation.target,
        "method": navigation.method,
        "body": navigation.body,
        "body_content_type": navigation.body_content_type,
        "location": navigation.location,
        "replace_history": navigation.replace_history,
        "object_url": navigation.object_url,
    })
}

fn append_content_response_array<T: Serialize>(
    response: &mut Value,
    field: &str,
    additions: &T,
    limit: usize,
    operation: &'static str,
) -> Result<(), NativeEngineError> {
    let additions = serde_json::to_value(additions).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: format!("content-process {field} effects could not be encoded"),
    })?;
    let additions = additions
        .as_array()
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: format!("content-process {field} effects were not an array"),
        })?;
    if additions.is_empty() {
        return Ok(());
    }
    let object = response
        .as_object_mut()
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content-process mutation response was not an object".into(),
        })?;
    let values = object
        .entry(field.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: format!("content-process {field} effects were malformed"),
        })?;
    let total = values.len().saturating_add(additions.len());
    if total > limit {
        return Err(NativeEngineError::limit(operation, limit, total));
    }
    values.extend(additions.iter().cloned());
    Ok(())
}

pub(super) fn append_content_mutation_response_effects(
    response: &mut Value,
    mutation: &NativeContentMutation,
) -> Result<(), NativeEngineError> {
    let events = mutation
        .events
        .iter()
        .map(|event| {
            json!({
                "node_index": event.node_index,
                "kind": event_kind_text(event.kind),
            })
        })
        .collect::<Vec<_>>();
    append_content_response_array(
        response,
        "events",
        &events,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation events",
    )?;
    append_content_response_array(
        response,
        "mutation_history",
        &mutation.history,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation history",
    )?;
    append_content_response_array(
        response,
        "scroll_commands",
        &mutation.scroll_commands,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation scroll commands",
    )?;
    append_content_response_array(
        response,
        "dialogs",
        &mutation.dialogs,
        MAX_NATIVE_DIALOGS,
        "encode parent-brokered mutation dialogs",
    )?;
    append_content_response_array(
        response,
        "popups",
        &mutation.popups,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation popups",
    )?;
    append_content_response_array(
        response,
        "post_messages",
        &mutation.post_messages,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation postMessage requests",
    )?;
    append_content_response_array(
        response,
        "window_closes",
        &mutation.window_closes,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation window close requests",
    )?;
    append_content_response_array(
        response,
        "window_navigations",
        &mutation.window_navigations,
        MAX_NATIVE_EFFECTS,
        "encode parent-brokered mutation window navigation requests",
    )?;
    if let Some(navigation) = mutation.navigation.as_ref() {
        if response
            .get("navigation")
            .is_some_and(|value| !value.is_null())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one parent-brokered mutation cannot activate multiple navigations".into(),
            });
        }
        response["navigation"] = content_navigation_json(navigation);
    }
    Ok(())
}
