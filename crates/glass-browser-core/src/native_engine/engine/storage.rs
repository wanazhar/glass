use super::*;

impl NativeEngine {
    pub(super) fn reconcile_pending_content_cookie_changes(
        &mut self,
        operation: &'static str,
    ) -> Result<(), NativeEngineError> {
        let cookie_writes = self
            .content_process
            .as_mut()
            .map(NativeContentProcess::take_pending_cookie_writes)
            .transpose()?
            .unwrap_or_default();
        let mut pending_changes =
            self.pending_content_cookie_changes
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process cookie change queue is poisoned".into(),
                })?;
        // Applying the child's outstanding response-cookie journal must not
        // consume changes produced by a parent-brokered request in this turn.
        let existing_loader_changes = self.loader.take_cookie_changes();
        if let Err(error) = self.loader.apply_cookie_changes(&pending_changes) {
            self.loader.restore_cookie_changes(existing_loader_changes);
            return Err(error);
        }
        let reconciled_changes = self.loader.take_cookie_changes();
        self.loader.restore_cookie_changes(existing_loader_changes);
        self.loader.restore_cookie_changes(reconciled_changes);
        pending_changes.clear();
        drop(pending_changes);
        for write in cookie_writes {
            self.loader
                .set_document_cookie(&write.owner.document_url, &write.value)?;
        }
        Ok(())
    }

    pub(super) fn persist_pending_loader_cookie_changes(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        let changes = self.loader.take_cookie_changes();
        if changes.is_empty() {
            return Ok(());
        }
        if let Err(error) = self.publish_external_cookie_changes(&changes) {
            self.loader.restore_cookie_changes(changes);
            return Err(error);
        }
        Ok(())
    }

    /// Give the inline owner the same bounded worker Fetch stream turn that
    /// the content process already runs around page evaluations. The yield is
    /// intentional: local fixture streams are spawned tasks, so their next
    /// demand-driven event must get a scheduler opportunity before the owner
    /// polls the event channel.
    pub async fn storage_async(
        &mut self,
        scope: StorageScope,
        operation: StorageOperation,
    ) -> Result<std::collections::BTreeMap<String, String>, NativeEngineError> {
        self.require_running("storage")?;
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;

        if matches!(&scope, StorageScope::Cookies) {
            return match operation {
                StorageOperation::Read => {
                    let cookies = self.cookies_async().await?;
                    let mut entries = std::collections::BTreeMap::new();
                    for cookie in cookies {
                        entries.entry(cookie.name).or_insert(cookie.value);
                    }
                    Ok(entries)
                }
                StorageOperation::Clear => {
                    self.clear_cookies_async().await?;
                    Ok(std::collections::BTreeMap::new())
                }
                StorageOperation::Write { key, value } => {
                    let cookie_line = format!("{key}={value}");
                    if !is_network_url(&self.url) {
                        return Err(NativeEngineError::UnsupportedUrl {
                            reason: "semantic cookie writes require an HTTP(S) document".into(),
                        });
                    }
                    if self.content_process.is_some() {
                        let cookie_line = serde_json::to_string(&cookie_line).map_err(|_| {
                            NativeEngineError::Worker {
                                operation: "native cookie write".into(),
                                reason: "cookie assignment could not be encoded".into(),
                            }
                        })?;
                        self.evaluate_async(format!("document.cookie = {cookie_line}; true"))
                            .await?;
                    } else {
                        self.loader.set_document_cookie(&self.url, &cookie_line)?;
                        self.persist_local_web_storage()?;
                    }
                    let cookies = self.cookies_async().await?;
                    let mut entries = std::collections::BTreeMap::new();
                    for cookie in cookies {
                        entries.entry(cookie.name).or_insert(cookie.value);
                    }
                    Ok(entries)
                }
            };
        }

        let storage_name = match scope {
            StorageScope::Local => "localStorage",
            StorageScope::Session => "sessionStorage",
            StorageScope::Cookies => unreachable!("cookies are handled above"),
        };

        match operation {
            StorageOperation::Read => {}
            StorageOperation::Write { key, value } => {
                let key = serde_json::to_string(&key).map_err(|_| NativeEngineError::Worker {
                    operation: "native storage write".into(),
                    reason: "storage key could not be encoded".into(),
                })?;
                let value =
                    serde_json::to_string(&value).map_err(|_| NativeEngineError::Worker {
                        operation: "native storage write".into(),
                        reason: "storage value could not be encoded".into(),
                    })?;
                self.evaluate_async(format!(
                    "window.{storage_name}.setItem({key}, {value}); true"
                ))
                .await?;
            }
            StorageOperation::Clear => {
                self.evaluate_async(format!("window.{storage_name}.clear(); true"))
                    .await?;
            }
        }

        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let storage_key = storage_key(&self.url, &self.origin);
        Ok(self.web_storage.entries_for(
            match scope {
                StorageScope::Local => "local",
                StorageScope::Session => "session",
                StorageScope::Cookies => unreachable!("cookies are handled above"),
            },
            &storage_key,
        ))
    }

    /// Return cookies matching the current document URL, including HTTP-only
    /// entries that are not visible to `document.cookie`.
    pub async fn cookies_async(&mut self) -> Result<Vec<Cookie>, NativeEngineError> {
        self.require_running("cookies")?;
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        self.reconcile_pending_content_cookie_changes("read parent cookie authority")?;
        self.persist_pending_loader_cookie_changes()?;
        let profiles = self.loader.cookies_for_document(&self.url)?;
        profiles
            .into_iter()
            .map(public_cookie_from_profile)
            .collect()
    }

    /// Import cookies into the current native profile and refresh the active
    /// content realm. Cookie attributes are retained rather than flattened
    /// into a name/value map.
    pub async fn set_cookies_async(&mut self, cookies: &[Cookie]) -> Result<(), NativeEngineError> {
        self.require_running("set cookies")?;
        if cookies.len() > crate::native_engine::javascript::MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                crate::native_engine::javascript::MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                cookies.len(),
            ));
        }
        let profiles = cookies
            .iter()
            .map(profile_from_public_cookie)
            .collect::<Result<Vec<_>, _>>()?;
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        self.reconcile_pending_content_cookie_changes("set parent cookie authority")?;
        self.persist_pending_loader_cookie_changes()?;
        let mut loader = self.loader.clone();
        let cookie_change_checkpoint = loader.cookie_change_checkpoint();
        loader.set_cookie_profiles(&profiles)?;
        let cookie_changes = loader.cookie_changes_since(cookie_change_checkpoint);
        let document_cookie = loader.document_cookie(&self.url)?;
        if let Some(process) = self.content_process.as_mut() {
            process
                .sync_document_cookie_projection(&self.url, &document_cookie)
                .await?;
        }
        self.loader
            .commit_staged_cookie_state(loader, &cookie_changes)?;
        // The process-backed runtime skips local JavaScript persistence, so
        // publish API imports explicitly to the shared profile just as the
        // cookie-clear path does. Other live sessions consume this journal.
        let committed_changes = self.loader.take_cookie_changes();
        self.publish_external_cookie_changes(&committed_changes)?;
        Ok(())
    }

    pub async fn apply_cookie_changes_async(
        &mut self,
        changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
        self.apply_cookie_changes_with_persistence(changes, true)
            .await?;
        if self.content_process.is_some() {
            let changes = self.loader.take_cookie_changes();
            self.publish_external_cookie_changes(&changes)
        } else {
            self.persist_local_web_storage()
        }
    }

    pub async fn apply_cookie_changes_to_runtime_async(
        &mut self,
        changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
        self.apply_cookie_changes_with_persistence(changes, false)
            .await
    }

    pub(super) async fn apply_cookie_changes_with_persistence(
        &mut self,
        changes: &[NativeCookieChange],
        persist_profile: bool,
    ) -> Result<(), NativeEngineError> {
        self.require_running("apply native cookie changes")?;
        let mut loader = self.loader.clone();
        let cookie_change_checkpoint = loader.cookie_change_checkpoint();
        loader.apply_cookie_changes(changes)?;
        let cookie_changes = loader.cookie_changes_since(cookie_change_checkpoint);
        if !persist_profile {
            loader.take_cookie_changes();
        }
        let document_url = self.url.clone();
        let document_cookie = loader.document_cookie(&document_url)?;
        if let Some(process) = self.content_process.as_mut() {
            process
                .sync_document_cookie_projection(&document_url, &document_cookie)
                .await?;
        }
        self.loader
            .commit_staged_cookie_state(loader, &cookie_changes)?;
        Ok(())
    }

    /// Clear all cookies in the native profile and active content realm.
    pub async fn clear_cookies_async(&mut self) -> Result<(), NativeEngineError> {
        self.require_running("clear cookies")?;
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        self.reconcile_pending_content_cookie_changes("clear parent cookie authority")?;
        self.persist_pending_loader_cookie_changes()?;
        let mut loader = self.loader.clone();
        let cookie_change_checkpoint = loader.cookie_change_checkpoint();
        loader.clear_cookies();
        let cookie_changes = loader.cookie_changes_since(cookie_change_checkpoint);
        let document_cookie = loader.document_cookie(&self.url)?;
        if let Some(process) = self.content_process.as_mut() {
            process
                .sync_document_cookie_projection(&self.url, &document_cookie)
                .await?;
        }
        self.loader
            .commit_staged_cookie_state(loader, &cookie_changes)?;
        let clear_changes = self.loader.take_cookie_changes();
        self.publish_external_cookie_changes(&clear_changes)?;
        let mut pending =
            self.pending_content_cookie_changes
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "publish parent cookie clear".into(),
                    reason: "content process cookie change queue is poisoned".into(),
                })?;
        *pending = clear_changes;
        Ok(())
    }

    /// Return the oldest unresolved JavaScript dialog for the active page.
    pub(super) fn merge_external_cookie_changes(
        target: &mut Vec<NativeCookieChange>,
        changes: impl IntoIterator<Item = NativeCookieChange>,
    ) -> Result<(), NativeEngineError> {
        for change in changes {
            if let Some(existing) = target.iter_mut().find(|existing| {
                existing.name == change.name
                    && existing.domain == change.domain
                    && existing.path == change.path
            }) {
                *existing = change;
            } else {
                if target.len() >= MAX_NATIVE_COOKIE_CHANGE_BATCH {
                    return Err(NativeEngineError::limit(
                        "native pending external cookie changes",
                        MAX_NATIVE_COOKIE_CHANGE_BATCH,
                        target.len().saturating_add(1),
                    ));
                }
                target.push(change);
            }
        }
        Ok(())
    }

    pub(super) fn diff_cookie_profiles(
        before: &[NativeCookieProfileEntry],
        after: &[NativeCookieProfileEntry],
    ) -> Vec<NativeCookieChange> {
        let mut changes = Vec::new();
        for cookie in before {
            let after_cookie = after.iter().find(|candidate| {
                candidate.name == cookie.name
                    && candidate.domain == cookie.domain
                    && candidate.path == cookie.path
            });
            if after_cookie != Some(cookie) {
                changes.push(NativeCookieChange {
                    name: cookie.name.clone(),
                    domain: cookie.domain.clone(),
                    path: cookie.path.clone(),
                    cookie: after_cookie.cloned(),
                });
            }
        }
        for cookie in after {
            if !before.iter().any(|candidate| {
                candidate.name == cookie.name
                    && candidate.domain == cookie.domain
                    && candidate.path == cookie.path
            }) {
                changes.push(NativeCookieChange {
                    name: cookie.name.clone(),
                    domain: cookie.domain.clone(),
                    path: cookie.path.clone(),
                    cookie: Some(cookie.clone()),
                });
            }
        }
        changes
    }

    pub(super) fn sync_external_storage_events(&mut self) -> Result<(), NativeEngineError> {
        let journal = read_storage_event_journal(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            &mut self.storage_event_offset,
        )?;
        let mut external_cookie_changes = Vec::new();
        if journal.recovered {
            let profile_state = load_web_storage_profile(self.config.storage_path.as_deref())?;
            let indexed_db_state = load_indexed_db_profile(self.config.storage_path.as_deref())?;
            let cookie_profile = load_cookie_profile(self.config.storage_path.as_deref())?;
            let cookie_changes =
                Self::diff_cookie_profiles(&self.loader.cookie_profile(), &cookie_profile);
            Self::merge_external_cookie_changes(&mut external_cookie_changes, cookie_changes)?;
            self.web_storage.replace_profile_state(profile_state);
            self.indexed_db = indexed_db_state;
            self.storage_state_recovery_pending = true;
            self.indexed_db_state_delivery_pending = true;
            self.pending_external_storage_events.clear();
            if let Some(javascript) = self.javascript.as_ref() {
                javascript.replace_storage_state(self.web_storage.clone());
                javascript.set_indexed_db_state(
                    self.indexed_db
                        .origin(&storage_key(&self.url, &self.origin)),
                );
            }
        }
        let current_storage_key = storage_key(&self.url, &self.origin);
        let mut events = Vec::new();
        let mut indexed_db_changes = Vec::new();
        for record in journal.records {
            if record.writer_id == self.storage_writer_id {
                continue;
            }
            if let Some(event) = record.event
                && event.storage_key == current_storage_key
                && (event.scope == "local"
                    || (event.scope == "session"
                        && event.source_context_id == self.config.context_id))
            {
                events.push(event);
            }
            if !record.indexed_db_changes.is_empty() {
                apply_indexed_db_changes(&mut self.indexed_db, &record.indexed_db_changes)?;
                indexed_db_changes.extend(record.indexed_db_changes);
            }
            Self::merge_external_cookie_changes(
                &mut external_cookie_changes,
                record.cookie_changes,
            )?;
        }
        if !external_cookie_changes.is_empty() {
            self.loader.apply_cookie_changes(&external_cookie_changes)?;
            self.loader.take_cookie_changes();
            Self::merge_external_cookie_changes(
                &mut self.pending_external_cookie_changes,
                external_cookie_changes,
            )?;
        }
        if !indexed_db_changes.is_empty() {
            self.indexed_db_state_delivery_pending = true;
        }
        if events.is_empty() {
            return Ok(());
        }
        for event in &events {
            self.web_storage.apply_storage_event(event)?;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_storage_state(self.web_storage.clone());
            javascript.set_storage_events(events)?;
        } else {
            let next_len = self
                .pending_external_storage_events
                .len()
                .saturating_add(events.len());
            if next_len > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native pending storage events",
                    MAX_NATIVE_EFFECTS,
                    next_len,
                ));
            }
            self.pending_external_storage_events.extend(events);
        }
        Ok(())
    }

    pub(super) async fn deliver_pending_external_storage_events(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        if self.pending_external_storage_events.is_empty()
            && self.pending_external_cookie_changes.is_empty()
            && !self.storage_state_recovery_pending
            && !self.indexed_db_state_delivery_pending
        {
            return Ok(());
        }
        let events = std::mem::take(&mut self.pending_external_storage_events);
        let cookie_changes = std::mem::take(&mut self.pending_external_cookie_changes);
        let storage_state = self.web_storage.clone();
        let indexed_db_state = self.indexed_db.clone();
        let recovery_pending = self.storage_state_recovery_pending;
        let indexed_db_pending = self.indexed_db_state_delivery_pending;
        let document_url = self.url.clone();
        let document_cookie = if cookie_changes.is_empty() {
            None
        } else {
            Some(self.loader.document_cookie(&document_url)?)
        };
        if let Some(process) = self.content_process.as_mut() {
            if !process.refresh_health() {
                self.pending_external_storage_events = events;
                self.pending_external_cookie_changes = cookie_changes;
                return Err(NativeEngineError::worker_failure(
                    "content process storage events",
                    process
                        .failure_kind()
                        .unwrap_or(NativeWorkerFailureKind::Exited),
                    "content process is unavailable after a failed operation; navigate to recover it",
                ));
            }
            let result = async {
                if recovery_pending || indexed_db_pending {
                    process
                        .sync_storage_state(&storage_state, &indexed_db_state)
                        .await?;
                }
                if let Some(document_cookie) = document_cookie.as_deref() {
                    process
                        .sync_document_cookie_projection(&document_url, document_cookie)
                        .await?;
                }
                process.sync_storage_events(&events).await
            }
            .await;
            match result {
                Ok(()) => {
                    self.storage_state_recovery_pending = false;
                    self.indexed_db_state_delivery_pending = false;
                    Ok(())
                }
                Err(error) => {
                    self.pending_external_storage_events = events;
                    self.pending_external_cookie_changes = cookie_changes;
                    Err(error)
                }
            }
        } else if let Some(javascript) = self.javascript.as_ref() {
            if recovery_pending {
                javascript.replace_storage_state(storage_state);
            } else {
                javascript.set_storage_state(storage_state);
            }
            if indexed_db_pending || recovery_pending {
                javascript.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&self.url, &self.origin)),
                );
            }
            javascript.set_cookie_state(self.loader.document_cookie(&self.url)?);
            let result = if events.is_empty() {
                Ok(())
            } else {
                javascript.set_storage_events(events.clone())
            };
            if result.is_ok() {
                self.storage_state_recovery_pending = false;
                self.indexed_db_state_delivery_pending = false;
            } else {
                self.pending_external_storage_events = events;
            }
            result
        } else {
            self.pending_external_storage_events = events;
            Ok(())
        }
    }

    pub(crate) fn publish_external_cookie_changes(
        &self,
        changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
        save_web_storage_profile(
            self.config.storage_path.as_deref(),
            &self.web_storage,
            &[],
            &self.loader.cookie_profile(),
            changes,
            &self.indexed_db,
            &[],
        )?;
        append_storage_changes(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            &[],
            &[],
            changes,
        )
    }

    pub(super) fn publish_content_state(
        &mut self,
        events: &[NativeStorageEvent],
        indexed_db_changes: &[NativeIndexedDbChange],
    ) -> Result<(), NativeEngineError> {
        for event in events {
            self.web_storage.apply_storage_event(event)?;
        }
        apply_indexed_db_changes(&mut self.indexed_db, indexed_db_changes)?;
        append_storage_changes(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            events,
            indexed_db_changes,
            &[],
        )?;
        Ok(())
    }

    pub(super) fn persist_local_web_storage(&mut self) -> Result<(), NativeEngineError> {
        if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            return Ok(());
        }
        let (storage_changes, indexed_db_changes) = if let Some(javascript) =
            self.javascript.as_ref()
        {
            let cookie_updates = javascript.take_cookie_updates();
            self.web_storage = javascript.storage_state();
            let indexed_db_key = storage_key(&self.url, &self.origin);
            let before_indexed_db = self.indexed_db.origin(&indexed_db_key);
            let after_indexed_db = javascript.indexed_db_state();
            let indexed_db_changes =
                diff_indexed_db_changes(&indexed_db_key, &before_indexed_db, &after_indexed_db)?;
            self.indexed_db
                .replace_origin(indexed_db_key, after_indexed_db)?;
            for value in cookie_updates {
                self.loader.set_document_cookie(&self.url, &value)?;
            }
            (javascript.take_storage_changes(), indexed_db_changes)
        } else {
            (Vec::new(), Vec::new())
        };
        let cookie_state = self.loader.cookie_profile();
        let cookie_changes = self.loader.take_cookie_changes();
        save_web_storage_profile(
            self.config.storage_path.as_deref(),
            &self.web_storage,
            &storage_changes,
            &cookie_state,
            &cookie_changes,
            &self.indexed_db,
            &indexed_db_changes,
        )?;
        append_storage_changes(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            &storage_changes,
            &indexed_db_changes,
            &cookie_changes,
        )?;
        Ok(())
    }

    pub(super) fn persist_local_script_state(&mut self) -> Result<(), NativeEngineError> {
        self.persist_local_web_storage()
    }
}
