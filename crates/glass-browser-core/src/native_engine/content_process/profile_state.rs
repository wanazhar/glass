use super::*;

pub(super) fn sync_content_runtime_state(
    runtime: Option<&NativeJavaScriptRuntime>,
    storage_state: &mut NativeWebStorageState,
    indexed_db_state: &mut NativeIndexedDbState,
    resource_loader: &mut Option<NativeResourceLoader>,
    document_url: Option<&str>,
    document_origin: Option<&NativeOrigin>,
) -> Result<
    (
        Vec<NativeStorageEvent>,
        Vec<NativeIndexedDbChange>,
        Vec<NativeDialog>,
        Vec<NativePopupRequest>,
        Vec<NativePostMessageRequest>,
        Vec<NativeWindowCloseRequest>,
        Vec<NativeWindowNavigationRequest>,
        String,
    ),
    NativeEngineError,
> {
    let Some(runtime) = runtime else {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            String::new(),
        ));
    };
    if let Some(loader) = resource_loader.as_mut()
        && let Some(updated_loader) = runtime.take_sync_xhr_loader()
    {
        loader.merge_fetch_task_state(updated_loader)?;
    }
    let before_indexed_db = indexed_db_state.clone();
    *storage_state = runtime.storage_state();
    let indexed_db_changes =
        if let (Some(document_url), Some(document_origin)) = (document_url, document_origin) {
            let key = storage_key(document_url, document_origin);
            let before = before_indexed_db.origin(&key);
            let after = runtime.indexed_db_state();
            indexed_db_state.replace_origin(key.clone(), after.clone())?;
            diff_indexed_db_changes(&key, &before, &after)?
        } else {
            Vec::new()
        };
    Ok((
        runtime.take_storage_changes(),
        indexed_db_changes,
        runtime.take_dialog_events(),
        runtime.take_popup_events(),
        runtime.take_post_message_events(),
        runtime.take_window_close_events(),
        runtime.take_window_navigation_events(),
        runtime.window_name(),
    ))
}

pub(super) fn take_content_cookie_writes(
    runtime: Option<&NativeJavaScriptRuntime>,
    context_id: &str,
    frame_id: &str,
    generation: Option<u32>,
    document_url: Option<&str>,
) -> Result<Vec<NativeContentCookieWrite>, NativeEngineError> {
    let Some(runtime) = runtime else {
        return Ok(Vec::new());
    };
    let values = runtime.take_cookie_updates();
    if values.is_empty() {
        return Ok(Vec::new());
    }
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content process document.cookie writes",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let generation = generation
        .filter(|generation| *generation != 0)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "content process document.cookie writes",
                NativeWorkerFailureKind::Protocol,
                "cookie write has no active document generation",
            )
        })?;
    let document_url = document_url.ok_or_else(|| {
        NativeEngineError::worker_failure(
            "content process document.cookie writes",
            NativeWorkerFailureKind::Protocol,
            "cookie write has no active document URL",
        )
    })?;
    validate_context_id(context_id)?;
    validate_context_id(frame_id)?;
    validate_url_text("content process cookie write owner URL", document_url)?;
    let owner = NativeContentCookieOwner {
        context_id: context_id.to_owned(),
        frame_id: frame_id.to_owned(),
        generation,
        document_url: document_url.to_owned(),
    };
    let mut total_bytes = 0usize;
    for value in &values {
        if value.len() > crate::limits::MAX_TEXT_BYTES {
            return Err(NativeEngineError::limit(
                "content process document.cookie write",
                crate::limits::MAX_TEXT_BYTES,
                value.len(),
            ));
        }
        total_bytes = total_bytes.saturating_add(value.len());
        if total_bytes > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "content process document.cookie write journal",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                total_bytes,
            ));
        }
    }
    let writes = values
        .into_iter()
        .map(|value| NativeContentCookieWrite {
            owner: owner.clone(),
            value,
        })
        .collect();
    Ok(writes)
}

pub(super) fn persist_content_profile(
    storage_path: Option<&Path>,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    storage_events: &[NativeStorageEvent],
    indexed_db_changes: &[NativeIndexedDbChange],
    service_workers: &mut NativeServiceWorkerRegistry,
    resource_loader: &mut Option<NativeResourceLoader>,
) -> Result<Vec<NativeCookieChange>, NativeEngineError> {
    let cookie_changes = resource_loader
        .as_mut()
        .map(NativeResourceLoader::take_cookie_changes)
        .unwrap_or_default();
    if !cookie_changes.is_empty() {
        return Err(NativeEngineError::worker_failure(
            "content process cookie authority",
            NativeWorkerFailureKind::Protocol,
            "content loader changed cookie state despite having no cookie authority",
        ));
    }
    let cookie_changes = coalesce_cookie_changes(cookie_changes, "content process cookie changes")?;
    save_content_web_storage_profile(
        storage_path,
        storage_state,
        storage_events,
        indexed_db_state,
        indexed_db_changes,
    )?;
    let registration_profiles = service_workers.registration_profiles();
    let registration_changes = service_workers.registration_changes().clone();
    save_service_worker_cache_profile(
        storage_path,
        service_workers.cache_state(),
        &registration_profiles,
        &registration_changes,
    )?;
    service_workers.clear_registration_changes();
    Ok(cookie_changes)
}

pub(super) fn coalesce_cookie_changes(
    changes: Vec<NativeCookieChange>,
    operation: &'static str,
) -> Result<Vec<NativeCookieChange>, NativeEngineError> {
    let mut coalesced = Vec::new();
    merge_cookie_change_batch(&mut coalesced, changes, operation)?;
    Ok(coalesced)
}

pub(super) fn merge_cookie_change_batch(
    target: &mut Vec<NativeCookieChange>,
    changes: Vec<NativeCookieChange>,
    operation: &'static str,
) -> Result<(), NativeEngineError> {
    let mut merged = target.clone();
    for change in changes {
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing.name == change.name
                && existing.domain == change.domain
                && existing.path == change.path
        }) {
            *existing = change;
        } else {
            if merged.len() >= MAX_NATIVE_COOKIE_CHANGE_BATCH {
                return Err(NativeEngineError::limit(
                    operation,
                    MAX_NATIVE_COOKIE_CHANGE_BATCH,
                    merged.len().saturating_add(1),
                ));
            }
            merged.push(change);
        }
    }
    *target = merged;
    Ok(())
}

pub(super) fn refresh_content_runtime_cookie(
    runtime: Option<&NativeJavaScriptRuntime>,
    resource_loader: Option<&NativeResourceLoader>,
    document_url: Option<&str>,
) -> Result<(), NativeEngineError> {
    if let (Some(runtime), Some(loader), Some(document_url)) =
        (runtime, resource_loader, document_url)
    {
        runtime.set_cookie_state(loader.document_cookie(document_url)?);
    }
    Ok(())
}
