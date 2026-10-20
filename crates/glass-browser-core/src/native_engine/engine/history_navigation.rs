use super::*;

impl NativeEngine {
    /// Clamp a history entry's saved scroll state against the document being
    /// activated. Same-document mutations can remove a scroller, and a full
    /// resource traversal can rebuild the DOM with different bounds; both
    /// cases should restore the usable portion of the saved state instead of
    /// making the next layout fail.
    pub(super) fn restore_history_scroll_state(
        &self,
        document: &NativeDocument,
        saved_scroll: NativePoint,
        saved_nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) -> Result<(NativePoint, BTreeMap<u32, NativePoint>), NativeEngineError> {
        let layout = document.layout(self.config.viewport)?;
        let max_scroll = layout.max_scroll_offset();
        let scroll_offset = NativePoint {
            x: saved_scroll.x.min(max_scroll.x),
            y: saved_scroll.y.min(max_scroll.y),
        };
        let mut nested_scroll_offsets = BTreeMap::new();
        for (&node_index, &saved_offset) in saved_nested_scroll_offsets {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            let Some(container) = layout.scroll_container_for(node_id) else {
                continue;
            };
            let max_scroll = container.max_scroll_offset();
            let offset = NativePoint {
                x: saved_offset.x.min(max_scroll.x),
                y: saved_offset.y.min(max_scroll.y),
            };
            if offset.x != 0 || offset.y != 0 {
                nested_scroll_offsets.insert(node_index, offset);
            }
        }
        Ok((scroll_offset, nested_scroll_offsets))
    }

    pub(super) fn commit_same_document_navigation(
        &mut self,
        url: String,
        history_commit: HistoryCommit,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let old_url = self.url.clone();
        self.document.set_css_target_from_url(&url)?;
        let (saved_scroll, saved_nested_scroll_offsets) = match &history_commit {
            HistoryCommit::Push | HistoryCommit::Replace => (
                self.fragment_scroll_offset(&url)?,
                self.nested_scroll_offsets.clone(),
            ),
            HistoryCommit::Activate(index) => {
                let entry =
                    self.history
                        .entry(*index)
                        .ok_or_else(|| NativeEngineError::Scheduler {
                            reason: "history target is no longer available".into(),
                        })?;
                (entry.scroll_offset, entry.nested_scroll_offsets.clone())
            }
        };
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &self.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        self.run_commit_task(
            NativeTask::CommitSameDocumentNavigation,
            "same-document navigation",
        )?;
        let revision = self.next_revision()?;
        self.document.set_revision(revision);
        self.url = url.clone();
        self.scroll_offset = scroll_offset;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.sync_javascript_scroll_offset();
        self.revision = revision;
        let traversing_history = matches!(&history_commit, HistoryCommit::Activate(_));
        match history_commit {
            HistoryCommit::Push => self.history.push_same_document(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
            ),
            HistoryCommit::Replace => self.history.replace_current_with_state(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
                serde_json::Value::Null,
                true,
            ),
            HistoryCommit::Activate(index) => {
                self.history.activate(index, revision).ok_or_else(|| {
                    NativeEngineError::Scheduler {
                        reason: "history entry disappeared during same-document traversal".into(),
                    }
                })?;
                self.history
                    .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            }
        }
        self.sync_javascript_history();
        let mut navigation = if traversing_history {
            self.dispatch_local_pop_state()?
        } else {
            None
        };
        let hash_navigation =
            if old_url != self.url && without_fragment(&old_url) == without_fragment(&self.url) {
                self.dispatch_local_hash_change(&old_url, &self.url.clone())?
            } else {
                None
            };
        if let Some(next_navigation) = hash_navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple same-document lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        self.persist_local_web_storage()?;
        Ok(navigation)
    }

    pub(super) async fn commit_same_document_navigation_async(
        &mut self,
        url: String,
        history_commit: HistoryCommit,
        worker: &NativeRuntimeWorker,
        page_navigation_handoffs: usize,
    ) -> Result<(), NativeEngineError> {
        let old_url = self.url.clone();
        self.document.set_css_target_from_url(&url)?;
        let (saved_scroll, saved_nested_scroll_offsets) = match &history_commit {
            HistoryCommit::Push | HistoryCommit::Replace => (
                self.fragment_scroll_offset(&url)?,
                self.nested_scroll_offsets.clone(),
            ),
            HistoryCommit::Activate(index) => {
                let entry =
                    self.history
                        .entry(*index)
                        .ok_or_else(|| NativeEngineError::Scheduler {
                            reason: "history target is no longer available".into(),
                        })?;
                (entry.scroll_offset, entry.nested_scroll_offsets.clone())
            }
        };
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &self.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        self.run_commit_task_async(
            NativeTask::CommitSameDocumentNavigation,
            "same-document navigation",
            worker,
        )
        .await?;
        let revision = self.next_revision()?;
        self.document.set_revision(revision);
        self.url = url.clone();
        self.scroll_offset = scroll_offset;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.sync_javascript_scroll_offset();
        self.revision = revision;
        let traversing_history = matches!(&history_commit, HistoryCommit::Activate(_));
        match history_commit {
            HistoryCommit::Push => self.history.push_same_document(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
            ),
            HistoryCommit::Replace => self.history.replace_current_with_state(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
                serde_json::Value::Null,
                true,
            ),
            HistoryCommit::Activate(index) => {
                self.history.activate(index, revision).ok_or_else(|| {
                    NativeEngineError::Scheduler {
                        reason: "history entry disappeared during same-document traversal".into(),
                    }
                })?;
                self.history
                    .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            }
        }
        self.sync_javascript_history();
        self.sync_content_scroll_offsets_async().await?;
        self.sync_content_history_async().await?;
        let mut navigation = if traversing_history {
            self.dispatch_content_events_async(&[NativeEventKind::PopState], false)
                .await?
        } else {
            None
        };
        let new_url = self.url.clone();
        let hash_navigation =
            if old_url != new_url && without_fragment(&old_url) == without_fragment(&new_url) {
                if self
                    .content_process
                    .as_mut()
                    .is_some_and(NativeContentProcess::refresh_health)
                {
                    self.dispatch_content_hash_change_async(&old_url, &new_url)
                        .await?
                } else {
                    self.dispatch_local_hash_change(&old_url, &new_url)?
                }
            } else {
                None
            };
        if let Some(next_navigation) = hash_navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple same-document lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        self.persist_local_web_storage()?;
        if let Some(navigation) = navigation {
            if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                return Err(NativeEngineError::limit(
                    "page navigation handoffs",
                    MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                    page_navigation_handoffs.saturating_add(1),
                ));
            }
            Box::pin(self.navigate_script_navigation_async(
                NativeContentNavigation {
                    node_index: 0,
                    href: navigation.url,
                    submitter_node_index: None,
                    target: "_self".into(),
                    method: NativeNavigationMethod::Get,
                    body: None,
                    body_content_type: None,
                    location: true,
                    replace_history: navigation.replace_history,
                    object_url: None,
                },
                page_navigation_handoffs + 1,
            ))
            .await?;
        }
        Ok(())
    }

    pub(super) fn commit_history_navigation(
        &mut self,
        prepared: PreparedNavigation,
        history_index: usize,
    ) -> Result<(), NativeEngineError> {
        self.persist_local_web_storage()?;
        if self.history.entry(history_index).is_none() {
            return Err(NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            });
        }
        let (saved_scroll, saved_nested_scroll_offsets) = self
            .history
            .entry(history_index)
            .map(|entry| (entry.scroll_offset, entry.nested_scroll_offsets.clone()))
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &prepared.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        let dialogs = prepared.dialogs;
        self.run_commit_task(NativeTask::TraverseHistory, "history traversal")?;
        let revision = prepared.document.revision();
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.javascript = None;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        self.history
            .activate(history_index, revision)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target disappeared during traversal".into(),
            })?;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.flush_pending_lifecycle_effects();
        Ok(())
    }

    pub(super) async fn commit_history_navigation_async(
        &mut self,
        prepared: PreparedNavigation,
        history_index: usize,
        worker: &NativeRuntimeWorker,
    ) -> Result<(), NativeEngineError> {
        self.persist_local_web_storage()?;
        if self.history.entry(history_index).is_none() {
            return Err(NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            });
        }
        let (saved_scroll, saved_nested_scroll_offsets) = self
            .history
            .entry(history_index)
            .map(|entry| (entry.scroll_offset, entry.nested_scroll_offsets.clone()))
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &prepared.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        let dialogs = prepared.dialogs;
        self.run_commit_task_async(NativeTask::TraverseHistory, "history traversal", worker)
            .await?;
        let revision = prepared.document.revision();
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.javascript = None;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        self.history
            .activate(history_index, revision)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target disappeared during traversal".into(),
            })?;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.sync_content_scroll_offsets_async().await?;
        self.sync_content_history_async().await?;
        self.flush_pending_lifecycle_effects();
        Ok(())
    }

    pub(super) fn traverse_history(
        &mut self,
        direction: NativeHistoryDirection,
        operation: &str,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.require_running(operation)?;
        let Some(history_index) = self.history.target_index(direction) else {
            return Ok(None);
        };
        let target_url = self
            .history
            .entry(history_index)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?
            .url
            .clone();
        let same_document_target = self.history.is_same_document(history_index)
            || self.is_same_document_navigation(&target_url);
        if !same_document_target {
            if self.outgoing_lifecycle_dispatch_depth > 0 {
                return Err(NativeEngineError::invalid(
                    "native history traversal",
                    "cross-document traversal cannot re-enter an outgoing lifecycle callback",
                ));
            }
            if self.content_process.is_some() {
                return Err(NativeEngineError::invalid(
                    "synchronous native history traversal",
                    "process-backed cross-document history requires the asynchronous native history API",
                ));
            }
        }
        if !self.allows_top_level_navigation(&target_url)? {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if same_document_target {
            if let Some(navigation) = self.commit_same_document_navigation(
                target_url,
                HistoryCommit::Activate(history_index),
            )? {
                self.navigate_page_script_sync(navigation, 1)?;
            }
            return Ok(Some(self.snapshot_unchecked()));
        }
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(Some(self.snapshot_unchecked()));
        }
        let current_history_index = self.history.current_index();
        let target_document_id = self
            .history
            .entry(history_index)
            .map(|entry| entry.document_id)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let (beforeunload_allowed, beforeunload_navigation) = if self.javascript.is_some() {
            self.dispatch_local_before_unload()?
        } else {
            (true, None)
        };
        if !beforeunload_allowed && self.document_has_sticky_activation {
            return Err(Self::synchronous_beforeunload_error());
        }
        let lifecycle_navigation = if self.javascript.is_some() {
            self.dispatch_local_navigation_lifecycle()?
        } else {
            None
        };
        if beforeunload_navigation.is_some() && lifecycle_navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "multiple outgoing lifecycle navigations are not supported".into(),
            });
        }
        if let Some(navigation) = beforeunload_navigation.or(lifecycle_navigation) {
            self.navigate_lifecycle_handoff_sync(navigation)?;
            return Ok(Some(self.snapshot_unchecked()));
        }
        if self.history.current_index() != current_history_index
            || self.history.entry(history_index).is_none_or(|entry| {
                entry.document_id != target_document_id || entry.url != target_url
            })
        {
            return Ok(Some(self.snapshot_unchecked()));
        }
        self.persist_local_web_storage()?;
        let resource = self.loader.load(&target_url)?;
        if self.is_same_document_navigation(&resource.url) {
            if let Some(navigation) = self.commit_same_document_navigation(
                resource.url,
                HistoryCommit::Activate(history_index),
            )? {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation(&resource.url)? {
            return Ok(Some(self.snapshot_unchecked()));
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            self.commit_history_navigation(prepared, history_index)?;
        }
        Ok(Some(self.snapshot_unchecked()))
    }

    pub(super) fn traverse_history_delta(&mut self, delta: i32) -> Result<(), NativeEngineError> {
        let steps = delta.unsigned_abs() as usize;
        if steps > MAX_NATIVE_HISTORY_DELTA as usize {
            return Err(NativeEngineError::limit(
                "native history traversal delta",
                MAX_NATIVE_HISTORY_DELTA as usize,
                steps,
            ));
        }
        let direction = if delta < 0 {
            NativeHistoryDirection::Back
        } else {
            NativeHistoryDirection::Forward
        };
        for _ in 0..steps {
            if self
                .traverse_history(direction, "history traversal")?
                .is_none()
            {
                break;
            }
        }
        Ok(())
    }

    pub(super) async fn traverse_history_async(
        &mut self,
        direction: NativeHistoryDirection,
        operation: &str,
        dispatch_lifecycle: bool,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.require_running(operation)?;
        let Some(history_index) = self.history.target_index(direction) else {
            return Ok(None);
        };
        let target_url = self
            .history
            .entry(history_index)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?
            .url
            .clone();
        let target_document_id = self
            .history
            .entry(history_index)
            .map(|entry| entry.document_id)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let same_document_target = self.history.is_same_document(history_index)
            || self.is_same_document_navigation(&target_url);
        if !same_document_target && self.outgoing_lifecycle_dispatch_depth > 0 {
            return Err(NativeEngineError::invalid(
                "native history traversal",
                "cross-document traversal cannot re-enter an outgoing lifecycle callback",
            ));
        }
        let allowed = self
            .allows_top_level_navigation_async(&target_url, true)
            .await?;
        if !allowed {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if same_document_target {
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    target_url,
                    HistoryCommit::Activate(history_index),
                    &worker,
                    0,
                )
                .await?;
            } else {
                if let Some(navigation) = self.commit_same_document_navigation(
                    target_url,
                    HistoryCommit::Activate(history_index),
                )? {
                    self.navigate_page_script_sync(navigation, 1)?;
                }
            }
            return Ok(Some(self.snapshot_unchecked()));
        }

        let history_commit = HistoryCommit::Activate(history_index);
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(Some(self.snapshot_unchecked()));
        }
        let current_history_index = self.history.current_index();
        let (lifecycle_allowed, lifecycle_navigation) = if dispatch_lifecycle {
            self.dispatch_navigation_lifecycle_async().await?
        } else {
            (true, None)
        };
        if !lifecycle_allowed {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if let Some(navigation) = lifecycle_navigation {
            return self
                .navigate_request_async_with_lifecycle(navigation, 0, false, None)
                .await
                .map(Some);
        }
        if self.history.current_index() != current_history_index
            || self.history.entry(history_index).is_none_or(|entry| {
                entry.document_id != target_document_id || entry.url != target_url
            })
        {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if is_network_url(&target_url) {
            let referrer = Some(self.url.clone());
            let referrer_policy = self.document.document_referrer_policy();
            self.ensure_content_process().await?;
            let Some((content, history_commit, page_navigation_handoffs, initial_url)) = self
                .load_content_with_page_navigation(
                    NativeNavigationRequest::get(target_url),
                    referrer,
                    referrer_policy,
                    history_commit,
                    0,
                    None,
                )
                .await?
            else {
                return Ok(Some(self.snapshot_unchecked()));
            };
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation_after_redirect(&initial_url, &content.url)?
            {
                return Ok(Some(self.snapshot_unchecked()));
            }
            self.commit_content_process().await?;
            if let Some(worker) = self.runtime_worker.clone() {
                return Ok(Some(
                    self.navigate_content_async(
                        content,
                        &worker,
                        history_commit,
                        page_navigation_handoffs,
                        &initial_url,
                    )
                    .await?,
                ));
            }
            return Ok(Some(self.navigate_content(
                content,
                &initial_url,
                history_commit,
            )?));
        }

        self.content_process.take();
        let resource = self.loader.load_async(&target_url).await?;
        if let Some(worker) = self.runtime_worker.clone() {
            return Ok(Some(
                self.navigate_resource_async(resource, &worker, history_commit, 0)
                    .await?,
            ));
        }
        Ok(Some(self.navigate_resource(resource, history_commit)?))
    }

    pub(super) async fn traverse_history_delta_async(
        &mut self,
        delta: i32,
    ) -> Result<(), NativeEngineError> {
        let steps = delta.unsigned_abs() as usize;
        if steps > MAX_NATIVE_HISTORY_DELTA as usize {
            return Err(NativeEngineError::limit(
                "native history traversal delta",
                MAX_NATIVE_HISTORY_DELTA as usize,
                steps,
            ));
        }
        let direction = if delta < 0 {
            NativeHistoryDirection::Back
        } else {
            NativeHistoryDirection::Forward
        };
        for _ in 0..steps {
            if self
                .traverse_history_async(direction, "history traversal", true)
                .await?
                .is_none()
            {
                break;
            }
        }
        Ok(())
    }
}
