use super::*;

pub(super) fn read_script_commands<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let json: String = ctx
        .eval("JSON.stringify(globalThis.__glassHostCommands || [])")
        .map_err(|_| NativeEngineError::Worker {
            operation: "collect JavaScript host commands".into(),
            reason: "native JavaScript host commands could not be collected".into(),
        })?;
    if json.len() > MAX_NATIVE_SCRIPT_COMMAND_BYTES {
        return Err(NativeEngineError::limit(
            "script host commands",
            MAX_NATIVE_SCRIPT_COMMAND_BYTES,
            json.len(),
        ));
    }
    let commands = serde_json::from_str::<Vec<NativeScriptCommand>>(&json).map_err(|_| {
        NativeEngineError::Worker {
            operation: "decode JavaScript host commands".into(),
            reason: "native JavaScript host commands were invalid".into(),
        }
    })?;
    if commands.len() > super::super::interaction::MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "script host commands",
            super::super::interaction::MAX_NATIVE_EFFECTS,
            commands.len(),
        ));
    }
    Ok(commands)
}

pub(super) fn encode_native_message_payload(
    payload: &serde_json::Value,
    operation: &str,
) -> Result<Vec<u8>, NativeEngineError> {
    let encoded = serde_json::to_vec(payload).map_err(|_| NativeEngineError::Worker {
        operation: format!("serialize {operation}"),
        reason: "native message payload could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
        return Err(NativeEngineError::limit(
            operation,
            MAX_NATIVE_POST_MESSAGE_BYTES,
            encoded.len(),
        ));
    }
    Ok(encoded)
}

pub(crate) fn validate_native_message_payload(
    payload: &serde_json::Value,
    operation: &str,
) -> Result<(), NativeEngineError> {
    encode_native_message_payload(payload, operation).map(|_| ())
}

pub(crate) fn validate_native_object_url_transfers(
    transfers: &[NativeObjectUrlTransfer],
) -> Result<(), NativeEngineError> {
    if transfers.len() > MAX_NATIVE_OBJECT_URL_TRANSFERS {
        return Err(NativeEngineError::limit(
            "native object URL transfers",
            MAX_NATIVE_OBJECT_URL_TRANSFERS,
            transfers.len(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total_bytes = 0usize;
    for transfer in transfers {
        validate_url_text("native object URL transfer", &transfer.href)?;
        if !transfer
            .href
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
        {
            return Err(NativeEngineError::invalid(
                "native object URL transfer",
                "must use the blob scheme",
            ));
        }
        if !seen.insert(&transfer.href) {
            return Err(NativeEngineError::invalid(
                "native object URL transfer",
                "URLs must be unique within one message",
            ));
        }
        if transfer.resource.body.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "native object URL transfer body",
                MAX_NATIVE_SCRIPT_BYTES,
                transfer.resource.body.len(),
            ));
        }
        if transfer
            .resource
            .content_type
            .as_ref()
            .is_some_and(|content_type| content_type.len() > MAX_NATIVE_SCRIPT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "native object URL transfer content type",
                MAX_NATIVE_SCRIPT_BYTES,
                transfer
                    .resource
                    .content_type
                    .as_ref()
                    .map_or(0, String::len),
            ));
        }
        total_bytes = total_bytes
            .saturating_add(transfer.href.len())
            .saturating_add(transfer.resource.body.len())
            .saturating_add(
                transfer
                    .resource
                    .content_type
                    .as_ref()
                    .map_or(0, String::len),
            );
        if total_bytes > MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native object URL transfers",
                MAX_NATIVE_POST_MESSAGE_BYTES,
                total_bytes,
            ));
        }
    }
    Ok(())
}

pub(super) fn set_native_timer_clock(
    ctx: &rquickjs::Ctx<'_>,
    now_ms: u64,
) -> Result<(), NativeEngineError> {
    ctx.globals()
        .set("__glassNativeTimerNowMs", now_ms as f64)
        .map_err(|error| NativeEngineError::Worker {
            operation: "set native timer clock".into(),
            reason: format!(
                "native timer clock could not enter the JavaScript realm: {}",
                CaughtError::from_error(ctx, error)
            ),
        })
}

pub(super) fn native_message_payload<'js>(
    ctx: &rquickjs::Ctx<'js>,
    payload: &serde_json::Value,
    operation: &str,
) -> Result<Value<'js>, NativeEngineError> {
    let encoded = encode_native_message_payload(payload, operation)?;
    ctx.json_parse(encoded)
        .map_err(|error| NativeEngineError::Worker {
            operation: format!("parse {operation}"),
            reason: format!(
                "native message payload could not enter the JavaScript realm: {}",
                CaughtError::from_error(ctx, error)
            ),
        })
}

pub(super) fn native_structured_payload<'js>(
    ctx: &rquickjs::Ctx<'js>,
    payload: &serde_json::Value,
    operation: &str,
) -> Result<Value<'js>, NativeEngineError> {
    let encoded = serde_json::to_vec(payload).map_err(|_| NativeEngineError::Worker {
        operation: format!("serialize {operation}"),
        reason: format!("{operation} could not be serialized"),
    })?;
    if encoded.len() > MAX_NATIVE_SCRIPT_COMMAND_BYTES {
        return Err(NativeEngineError::limit(
            operation,
            MAX_NATIVE_SCRIPT_COMMAND_BYTES,
            encoded.len(),
        ));
    }
    ctx.json_parse(encoded)
        .map_err(|error| NativeEngineError::Worker {
            operation: format!("parse {operation}"),
            reason: format!(
                "{operation} could not enter the JavaScript realm: {}",
                CaughtError::from_error(ctx, error)
            ),
        })
}

pub(super) fn dispatch_promise_rejections(
    ctx: &rquickjs::Ctx<'_>,
    event_type: &str,
    reasons: &[String],
    cancelable: bool,
) -> Result<(), NativeEngineError> {
    if reasons.is_empty() {
        return Ok(());
    }
    let event_type = native_structured_payload(
        ctx,
        &serde_json::Value::String(event_type.to_owned()),
        "native Promise rejection event type",
    )?;
    let reasons = native_structured_payload(
        ctx,
        &serde_json::Value::Array(
            reasons
                .iter()
                .map(|reason| serde_json::json!({ "reason": reason }))
                .collect(),
        ),
        "native Promise rejection events",
    )?;
    let dispatch: Function = ctx
        .globals()
        .get("__glassDispatchPromiseRejections")
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native Promise rejection events".into(),
            reason: format!(
                "native Promise rejection dispatcher was unavailable: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    dispatch
        .call::<_, Value>((event_type, reasons, cancelable))
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native Promise rejection events".into(),
            reason: format!(
                "native Promise rejection event dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}
