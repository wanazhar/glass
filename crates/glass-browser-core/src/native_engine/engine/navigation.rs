use super::*;

impl NativeEngine {
    pub(super) fn allows_frame_navigation(
        &self,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        let Some(document_url) = self.embedding_document_url.as_deref() else {
            return Ok(true);
        };
        validate_url_text("frame navigation URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "the frame policy owner URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "frame navigation URL is not valid URL syntax".into(),
            }
        })?;
        Ok(self
            .embedding_frame_sources
            .as_deref()
            .is_none_or(|groups| {
                groups
                    .iter()
                    .all(|sources| csp_sources_allow(Some(sources), &document_url, &target_url))
            }))
    }

    pub(super) fn allows_top_level_navigation(
        &mut self,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        let document_url = self.url.clone();
        let sources = self.document_navigate_to_sources.clone();
        self.allows_top_level_navigation_for_owner(
            &document_url,
            target_url,
            sources.as_deref(),
            true,
        )
    }

    pub(super) fn allows_top_level_navigation_silent(
        &self,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        let document_url = self.url.clone();
        let sources = self.document_navigate_to_sources.clone();
        if !self.loader.allows_navigation_silent(
            &document_url,
            target_url,
            NativeNavigationPolicyKind::NavigateTo,
        )? {
            return Ok(false);
        }
        Self::navigation_sources_allow(&document_url, target_url, sources.as_deref())
    }

    pub(super) async fn allows_top_level_navigation_async(
        &mut self,
        target_url: &str,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        let document_url = self.url.clone();
        let sources = self.document_navigate_to_sources.clone();
        self.allows_top_level_navigation_for_owner_async(
            &document_url,
            target_url,
            sources.as_deref(),
            report,
        )
        .await
    }

    pub(super) async fn allows_top_level_navigation_for_owner_async(
        &mut self,
        document_url: &str,
        target_url: &str,
        sources: Option<&[Vec<String>]>,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        // The process-backed document is the authority for response CSP. The
        // parent keeps the bounded source snapshot for enforced decisions, but
        // only the child can observe report-only violations for this document.
        // Copy it before the event bridge can mutate the engine state.
        let sources = sources.map(|groups| groups.to_vec());
        let child_ready = report
            && is_network_url(without_fragment(document_url))
            && self
                .content_process
                .as_mut()
                .is_some_and(|process| process.refresh_health() && process.has_current_document());
        let loader_allowed = if child_ready {
            let policy = {
                let process =
                    self.content_process
                        .as_mut()
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "content process navigation policy".into(),
                            reason: "native content process is not running".into(),
                        })?;
                Box::pin(process.check_navigation_policy(document_url, target_url, true)).await?
            };
            if !policy.csp_violations.is_empty() {
                let page_events = NativePageEventBatch {
                    csp_violations: policy.csp_violations,
                    ..NativePageEventBatch::default()
                };
                Box::pin(self.evaluate_page_with_events_async("undefined;".into(), page_events))
                    .await?;
            }
            policy.allowed
        } else if report {
            self.loader.allows_navigation(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        } else {
            self.loader.allows_navigation_silent(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        };
        if !loader_allowed {
            return Ok(false);
        }
        Self::navigation_sources_allow(document_url, target_url, sources.as_deref())
    }

    pub(super) fn allows_top_level_navigation_for_owner(
        &mut self,
        document_url: &str,
        target_url: &str,
        sources: Option<&[Vec<String>]>,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        let loader_allowed = if report {
            self.loader.allows_navigation(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        } else {
            self.loader.allows_navigation_silent(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        };
        if !loader_allowed {
            return Ok(false);
        }
        Self::navigation_sources_allow(document_url, target_url, sources)
    }

    pub(super) fn navigation_sources_allow(
        document_url: &str,
        target_url: &str,
        sources: Option<&[Vec<String>]>,
    ) -> Result<bool, NativeEngineError> {
        let Some(sources) = sources else {
            return Ok(true);
        };
        validate_url_text("CSP navigation owner URL", document_url)?;
        validate_url_text("CSP navigation target URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP navigation owner URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP navigation target URL is not valid URL syntax".into(),
            }
        })?;
        Ok(sources
            .iter()
            .all(|group| csp_sources_allow(Some(group), &document_url, &target_url)))
    }

    pub(super) fn allows_frame_navigation_after_redirect(
        &self,
        initial_url: &str,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        let Some(document_url) = self.embedding_document_url.as_deref() else {
            return Ok(true);
        };
        validate_url_text("initial frame navigation URL", initial_url)?;
        validate_url_text("frame navigation URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "the frame policy owner URL is not valid URL syntax".into(),
            }
        })?;
        let initial_url = url::Url::parse(without_fragment(initial_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "initial frame navigation URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "frame navigation URL is not valid URL syntax".into(),
            }
        })?;
        let redirected = initial_url != target_url;
        Ok(self
            .embedding_frame_sources
            .as_deref()
            .is_none_or(|groups| {
                groups.iter().all(|sources| {
                    csp_sources_allow(Some(sources), &document_url, &initial_url)
                        && if redirected {
                            csp_sources_allow_for_redirect(
                                Some(sources),
                                &document_url,
                                &target_url,
                            )
                        } else {
                            csp_sources_allow(Some(sources), &document_url, &target_url)
                        }
                })
            }))
    }

    pub const fn lifecycle(&self) -> NativeLifecycleState {
        self.lifecycle
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the current root viewport scroll offset.
    pub const fn scroll_offset(&self) -> NativePoint {
        self.scroll_offset
    }

    pub fn initialize(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => return Ok(()),
            NativeLifecycleState::Closed => {
                return Err(
                    self.lifecycle_error("initialize", "a closed native engine cannot be reopened")
                );
            }
            NativeLifecycleState::New => {}
        }
        let prepared = self.prepare_navigation(&self.config.initial_url.clone())?;
        self.runtime.start()?;
        let page_navigation = match self.commit_navigation(prepared, HistoryCommit::Push) {
            Ok(page_navigation) => page_navigation,
            Err(error) => {
                self.runtime.rollback_start()?;
                return Err(error);
            }
        };
        self.lifecycle = NativeLifecycleState::Running;
        if let Some(page_navigation) = page_navigation {
            self.navigate_page_script_sync(page_navigation, 1)?;
        }
        Ok(())
    }

    pub async fn initialize_async(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => return Ok(()),
            NativeLifecycleState::Closed => {
                return Err(
                    self.lifecycle_error("initialize", "a closed native engine cannot be reopened")
                );
            }
            NativeLifecycleState::New => {}
        }
        let initial_url = self.config.initial_url.clone();
        let mut content_process = if is_network_url(&initial_url) {
            Some(
                NativeContentProcess::spawn_with_event_notify(
                    self.config.storage_path.as_deref(),
                    self.loader.allowed_file_roots(),
                    self.dialog_control.clone(),
                    self.pending_content_cookie_changes.clone(),
                    self.content_process_event_notify.clone(),
                )
                .await?,
            )
        } else {
            None
        };
        if let Some(process) = content_process.as_mut() {
            process
                .start(
                    self.config.storage_path.as_deref(),
                    self.loader.allowed_file_roots(),
                    &self.config.context_id,
                    &self.frame_id,
                    &self.config.window_name,
                    self.config.opener_context_id.as_deref(),
                    &self.config.opener_window_name,
                    &self.config.opener_url,
                    self.frame_script_context.as_ref(),
                    &self.environment,
                    &self.service_worker_clients,
                    self.external_shared_worker_routing,
                )
                .await?;
        }
        let has_content_process = content_process.is_some();
        self.content_process = content_process;
        let mut use_content_process = has_content_process;
        let (prepared, history_commit, page_navigation_handoffs) = if has_content_process {
            let Some((content, history_commit, page_navigation_handoffs, initial_url)) = self
                .load_content_with_page_navigation(
                    NativeNavigationRequest::get(initial_url.clone()),
                    None,
                    self.document.document_referrer_policy(),
                    HistoryCommit::Push,
                    0,
                    None,
                )
                .await?
            else {
                let worker = NativeRuntimeWorker::spawn_shared(self.runtime.clone())?;
                worker.start().await?;
                self.runtime_worker = Some(worker);
                self.lifecycle = NativeLifecycleState::Running;
                return Ok(());
            };
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation_after_redirect(&initial_url, &content.url)?
            {
                self.content_process.take();
                use_content_process = false;
                (
                    self.prepare_navigation_resource(self.loader.load("about:blank")?)?,
                    HistoryCommit::Push,
                    0,
                )
            } else {
                (
                    self.prepare_navigation_content(content)?,
                    history_commit,
                    page_navigation_handoffs,
                )
            }
        } else {
            (
                self.prepare_navigation_async(&initial_url).await?,
                HistoryCommit::Push,
                0,
            )
        };
        let worker = NativeRuntimeWorker::spawn_shared(self.runtime.clone())?;
        worker.start().await?;
        if use_content_process {
            self.commit_content_process().await?;
        }
        let page_navigation = match self
            .commit_navigation_async(prepared, &worker, history_commit)
            .await
        {
            Ok(page_navigation) => page_navigation,
            Err(error) => {
                self.content_process.take();
                worker.rollback_start().await?;
                return Err(error);
            }
        };
        self.runtime_worker = Some(worker);
        self.lifecycle = NativeLifecycleState::Running;
        if let Some(page_navigation) = page_navigation {
            self.navigate_request_async(page_navigation, 1).await?;
        } else if use_content_process
            && let Some(page_navigation) = self.dispatch_content_page_show_async().await?
        {
            self.navigate_script_navigation_async(page_navigation, page_navigation_handoffs)
                .await?;
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.persist_local_web_storage()?;
                unregister_storage_reader(
                    self.config.storage_path.as_deref(),
                    &self.storage_writer_id,
                )?;
                unregister_service_worker_client_lease(
                    self.config.storage_path.as_deref(),
                    &self.service_worker_client_id(),
                    &self.storage_writer_id,
                )?;
                self.runtime.close()?;
                self.runtime_worker.take();
                self.content_process.take();
                self.lifecycle = NativeLifecycleState::Closed;
                Ok(())
            }
            NativeLifecycleState::New => Err(self.lifecycle_error(
                "close",
                "the native engine must be initialized before close",
            )),
            NativeLifecycleState::Closed => {
                Err(self.lifecycle_error("close", "the native engine is already closed"))
            }
        }
    }

    pub async fn close_async(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.persist_local_web_storage()?;
                let process_close_result = if let Some(process) = self.content_process.as_mut() {
                    if process.refresh_health() {
                        Some(process.close().await)
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.reconcile_pending_content_cookie_changes("close parent cookie authority")?;
                self.persist_pending_loader_cookie_changes()?;
                unregister_storage_reader(
                    self.config.storage_path.as_deref(),
                    &self.storage_writer_id,
                )?;
                unregister_service_worker_client_lease(
                    self.config.storage_path.as_deref(),
                    &self.service_worker_client_id(),
                    &self.storage_writer_id,
                )?;
                self.runtime.close()?;
                self.runtime_worker.take();
                self.content_process.take();
                self.lifecycle = NativeLifecycleState::Closed;
                if let Some(result) = process_close_result {
                    result?;
                }
                Ok(())
            }
            NativeLifecycleState::New => Err(self.lifecycle_error(
                "close",
                "the native engine must be initialized before close",
            )),
            NativeLifecycleState::Closed => {
                Err(self.lifecycle_error("close", "the native engine is already closed"))
            }
        }
    }

    pub fn navigate(
        &mut self,
        url: impl Into<String>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        self.pending_lifecycle_effects.clear();
        self.sync_external_storage_events()?;
        let url = url.into();
        if !self.allows_top_level_navigation(&url)? {
            return Ok(self.snapshot_unchecked());
        }
        let resource = self.loader.load(&url)?;
        if self.is_same_document_navigation(&resource.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(resource.url, HistoryCommit::Push)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation(&resource.url)? {
            return Ok(self.snapshot_unchecked());
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            if let Some(page_navigation) = self.commit_navigation(prepared, HistoryCommit::Push)? {
                self.navigate_page_script_sync(page_navigation, 1)?;
            }
        }
        Ok(self.snapshot_unchecked())
    }

    pub async fn navigate_async(
        &mut self,
        url: impl Into<String>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async(NativeNavigationRequest::get(url), 0)
            .await
    }

    /// Rebuild the current document owner and reload the active URL without
    /// replaying the operation that may have killed the content worker.
    ///
    /// Recovery deliberately skips page unload handlers: a failed worker has
    /// no trustworthy lifecycle state to deliver, and replaying an outgoing
    /// mutation could duplicate an external side effect. The current history
    /// entry is replaced, so recovery does not manufacture a new user-visible
    /// history step.
    pub async fn recover_async(&mut self) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("recover")?;
        if self
            .content_process
            .as_mut()
            .is_some_and(|process| !process.refresh_health())
        {
            self.content_process.take();
        }
        let mut navigation = NativeNavigationRequest::get(self.url.clone());
        navigation.replace_history = true;
        self.navigate_request_async_with_lifecycle(navigation, 0, false, None)
            .await
    }

    pub async fn navigate_request_async(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_cancellation(navigation, page_navigation_handoffs, None)
            .await
    }

    pub(super) async fn navigate_request_async_with_cancellation(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_policy(
            navigation,
            page_navigation_handoffs,
            true,
            cancellation,
        )
        .await
    }

    pub async fn navigate_request_async_after_lifecycle(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_policy(
            navigation,
            page_navigation_handoffs,
            false,
            cancellation,
        )
        .await
    }

    pub(super) async fn navigate_request_async_with_policy(
        &mut self,
        mut navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        dispatch_lifecycle: bool,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if let Some(target) = navigation.target.take() {
            let target_kind = target.to_ascii_lowercase();
            if !matches!(
                target_kind.as_str(),
                "_self" | "_parent" | "_top" | "_unfencedtop"
            ) || (target_kind != "_self" && self.frame_script_context.is_some())
            {
                if !self
                    .allows_top_level_navigation_async(&navigation.url, true)
                    .await?
                {
                    return Ok(self.snapshot_unchecked());
                }
                if self.queue_form_target_navigation(&target, &navigation)? {
                    return Ok(self.snapshot_unchecked());
                }
            }
        }
        self.navigate_request_async_with_lifecycle(
            navigation,
            page_navigation_handoffs,
            dispatch_lifecycle,
            cancellation,
        )
        .await
    }

    pub(super) async fn navigate_request_async_with_lifecycle(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        dispatch_lifecycle: bool,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        let same_document = self.is_same_document_navigation(&navigation.url);
        let allowed = self
            .allows_top_level_navigation_async(&navigation.url, true)
            .await?;
        if !allowed {
            return Ok(self.snapshot_unchecked());
        }
        if cancellation
            .as_ref()
            .is_some_and(NativeNavigationCancellation::is_cancelled)
        {
            return Err(NativeEngineError::NavigationCancelled);
        }
        if dispatch_lifecycle {
            self.pending_lifecycle_effects.clear();
        }
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let navigation_url = navigation.url.clone();
        let url = navigation_url.as_str();
        let history_commit = if navigation.replace_history {
            HistoryCommit::Replace
        } else {
            HistoryCommit::Push
        };
        if !same_document && !self.allows_frame_navigation(url)? {
            return Ok(self.snapshot_unchecked());
        }
        if !same_document && dispatch_lifecycle {
            let (allowed, lifecycle_navigation) =
                self.dispatch_navigation_lifecycle_async().await?;
            if !allowed {
                return Ok(self.snapshot_unchecked());
            }
            if let Some(lifecycle_navigation) = lifecycle_navigation {
                if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                    return Err(NativeEngineError::limit(
                        "page navigation handoffs",
                        MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                        page_navigation_handoffs.saturating_add(1),
                    ));
                }
                return Box::pin(self.navigate_request_async_with_lifecycle(
                    lifecycle_navigation,
                    page_navigation_handoffs + 1,
                    false,
                    cancellation,
                ))
                .await;
            }
        }
        if same_document && navigation.method == NativeNavigationMethod::Get {
            validate_url_text("navigation URL", url)?;
            if cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.begin_commit())
            {
                return Err(NativeEngineError::NavigationCancelled);
            }
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    navigation.url,
                    history_commit,
                    &worker,
                    page_navigation_handoffs,
                )
                .await?;
            } else {
                if let Some(navigation) =
                    self.commit_same_document_navigation(navigation.url, history_commit)?
                {
                    self.navigate_page_script_sync(navigation, page_navigation_handoffs + 1)?;
                }
            }
            return Ok(self.snapshot_unchecked());
        }
        if is_network_url(url) {
            let referrer = Some(self.url.clone());
            let referrer_policy = self.document.document_referrer_policy();
            let stage_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_start",
                phase = "start",
            );
            self.ensure_content_process().await?;
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_start",
                phase = "complete",
                elapsed_ms = stage_started.elapsed().as_millis() as u64,
            );
            let stage_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_load",
                phase = "start",
            );
            let Some((content, history_commit, page_navigation_handoffs, initial_url)) = self
                .load_content_with_page_navigation(
                    navigation,
                    referrer,
                    referrer_policy,
                    history_commit,
                    page_navigation_handoffs,
                    cancellation.clone(),
                )
                .await?
            else {
                if cancellation
                    .as_ref()
                    .is_some_and(NativeNavigationCancellation::is_cancelled)
                {
                    self.terminate_content_process_after_navigation_cancel()
                        .await?;
                    return Err(NativeEngineError::NavigationCancelled);
                }
                return Ok(self.snapshot_unchecked());
            };
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_load",
                phase = "complete",
                elapsed_ms = stage_started.elapsed().as_millis() as u64,
            );
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                self.terminate_content_process_after_navigation_cancel()
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation_after_redirect(&initial_url, &content.url)?
            {
                return Ok(self.snapshot_unchecked());
            }
            if cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.begin_commit())
            {
                self.terminate_content_process_after_navigation_cancel()
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
            let stage_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_process_commit",
                phase = "start",
            );
            self.commit_content_process().await?;
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_process_commit",
                phase = "complete",
                elapsed_ms = stage_started.elapsed().as_millis() as u64,
            );
            if let Some(worker) = self.runtime_worker.clone() {
                let stage_started = std::time::Instant::now();
                tracing::debug!(
                    target: "glass_browser_core::native_navigation",
                    stage = "content_apply",
                    phase = "start",
                );
                return self
                    .navigate_content_async(
                        content,
                        &worker,
                        history_commit,
                        page_navigation_handoffs,
                        &initial_url,
                    )
                    .await
                    .inspect(|_| {
                        tracing::debug!(
                            target: "glass_browser_core::native_navigation",
                            stage = "content_apply",
                            phase = "complete",
                            elapsed_ms = stage_started.elapsed().as_millis() as u64,
                        );
                    });
            }
            let stage_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_apply",
                phase = "start",
            );
            let result = self.navigate_content(content, &initial_url, history_commit);
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_apply",
                phase = "complete",
                elapsed_ms = stage_started.elapsed().as_millis() as u64,
            );
            return result;
        }
        self.content_process.take();
        let resource = self
            .loader
            .load_async_request_with_referrer(&navigation, None)
            .await?;
        if let Some(worker) = self.runtime_worker.clone() {
            self.navigate_resource_async(
                resource,
                &worker,
                history_commit,
                page_navigation_handoffs,
            )
            .await
        } else {
            if is_network_url(&resource.url) {
                self.commit_content_process().await?;
            }
            self.navigate_resource(resource, history_commit)
        }
    }

    pub(super) fn navigate_resource(
        &mut self,
        resource: NativeResource,
        history_commit: HistoryCommit,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&resource.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(resource.url, history_commit)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            if let Some(page_navigation) = self.commit_navigation(prepared, history_commit)? {
                self.navigate_page_script_sync(page_navigation, 1)?;
            }
        }
        Ok(self.snapshot_unchecked())
    }

    pub(super) async fn ensure_content_process(&mut self) -> Result<(), NativeEngineError> {
        if self
            .content_process
            .as_mut()
            .is_some_and(|process| !process.refresh_health())
        {
            self.content_process.take();
        }
        if self.content_process.is_none() {
            let stage_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_spawn",
                phase = "start",
            );
            let mut process = NativeContentProcess::spawn_with_event_notify(
                self.config.storage_path.as_deref(),
                self.loader.allowed_file_roots(),
                self.dialog_control.clone(),
                self.pending_content_cookie_changes.clone(),
                self.content_process_event_notify.clone(),
            )
            .await?;
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_spawn",
                phase = "complete",
                elapsed_ms = stage_started.elapsed().as_millis() as u64,
            );
            let stage_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_initialize",
                phase = "start",
            );
            process
                .start(
                    self.config.storage_path.as_deref(),
                    self.loader.allowed_file_roots(),
                    &self.config.context_id,
                    &self.frame_id,
                    &self.config.window_name,
                    self.config.opener_context_id.as_deref(),
                    &self.config.opener_window_name,
                    &self.config.opener_url,
                    self.frame_script_context.as_ref(),
                    &self.environment,
                    &self.service_worker_clients,
                    self.external_shared_worker_routing,
                )
                .await?;
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_initialize",
                phase = "complete",
                elapsed_ms = stage_started.elapsed().as_millis() as u64,
            );
            self.content_process = Some(process);
        }
        Ok(())
    }

    pub(super) async fn load_content_with_page_navigation(
        &mut self,
        mut navigation: NativeNavigationRequest,
        mut referrer: Option<String>,
        mut referrer_policy: NativeFetchReferrerPolicy,
        mut history_commit: HistoryCommit,
        mut page_navigation_handoffs: usize,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<Option<(NativeContentLoad, HistoryCommit, usize, String)>, NativeEngineError> {
        if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
            return Err(NativeEngineError::limit(
                "page navigation handoffs",
                MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                page_navigation_handoffs,
            ));
        }
        let mut policy_owner_url = self.url.clone();
        let mut policy_sources = self.document_navigate_to_sources.clone();
        let mut first_content_load = true;
        loop {
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                return Err(NativeEngineError::NavigationCancelled);
            }
            let initial_url = navigation.url.clone();
            let report = !first_content_load;
            if !self
                .allows_top_level_navigation_for_owner_async(
                    &policy_owner_url,
                    &initial_url,
                    policy_sources.as_deref(),
                    report,
                )
                .await?
            {
                return Ok(None);
            }
            first_content_load = false;
            self.reconcile_pending_content_cookie_changes("navigation parent cookie snapshot")?;
            self.persist_pending_loader_cookie_changes()?;
            self.request_ledger.begin()?;
            let client_id = self.service_worker_client_id();
            let service_worker_clients = self.service_worker_clients.clone();
            let load_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_load_exchange",
                phase = "start",
            );
            let content_result = match self.content_process.as_mut() {
                Some(process) => {
                    process
                        .load(
                            &navigation,
                            &self.config.limits,
                            self.config.viewport,
                            referrer.as_deref(),
                            referrer_policy,
                            &client_id,
                            &service_worker_clients,
                            cancellation.as_ref(),
                            &mut self.loader,
                        )
                        .await
                }
                None => Err(NativeEngineError::Worker {
                    operation: "content process load".into(),
                    reason: "native content process is not running".into(),
                }),
            };
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_worker_load_exchange",
                phase = "complete",
                elapsed_ms = load_started.elapsed().as_millis() as u64,
            );
            self.request_ledger.finish();
            self.reconcile_pending_content_cookie_changes(
                "content process navigation cookie changes",
            )?;
            self.persist_pending_loader_cookie_changes()?;
            let mut content = match content_result {
                Ok(NativeContentLoadResult::Loaded(content)) => *content,
                Ok(NativeContentLoadResult::Suspended(open_windows)) => {
                    if self.pending_service_worker_navigation.is_some() {
                        return Err(NativeEngineError::Worker {
                            operation: "native service worker navigation".into(),
                            reason: "another service worker navigation is already suspended".into(),
                        });
                    }
                    self.queue_service_worker_open_window_requests(open_windows)?;
                    self.pending_service_worker_navigation =
                        Some(NativePendingServiceWorkerNavigation {
                            initial_url,
                            history_commit,
                            page_navigation_handoffs,
                        });
                    return Ok(None);
                }
                Err(NativeEngineError::NavigationCancelled) => {
                    // `NativeContentProcess::load` has already killed and
                    // reaped the worker while settling its IPC exchange.
                    // Remove that dead handle so the next navigation starts a
                    // fresh content process instead of writing to a closed pipe.
                    self.content_process.take();
                    return Err(NativeEngineError::NavigationCancelled);
                }
                Err(error) => return Err(error),
            };
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                self.terminate_content_process_after_navigation_cancel()
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
            let apply_started = std::time::Instant::now();
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_effects_apply",
                phase = "start",
            );
            self.apply_content_load_effects(&mut content)?;
            tracing::debug!(
                target: "glass_browser_core::native_navigation",
                stage = "content_effects_apply",
                phase = "complete",
                elapsed_ms = apply_started.elapsed().as_millis() as u64,
            );
            // The response's own CSP governs navigations made by that
            // document, not the navigation that just loaded it. The parent
            // loader also learns response headers during this load, so
            // rechecking an unchanged URL here could apply the new document's
            // `navigate-to` policy to its own initial response.
            if content.url != initial_url
                && !self.allows_top_level_navigation_for_owner(
                    &policy_owner_url,
                    &content.url,
                    policy_sources.as_deref(),
                    false,
                )?
            {
                return Ok(None);
            }
            let Some(page_navigation) = content.navigation.clone() else {
                return Ok(Some((
                    content,
                    history_commit,
                    page_navigation_handoffs,
                    initial_url,
                )));
            };
            policy_sources = content.navigate_to_sources.clone();
            policy_owner_url = content.url.clone();
            if page_navigation_handoffs == MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                return Err(NativeEngineError::limit(
                    "page navigation handoffs",
                    MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                    page_navigation_handoffs.saturating_add(1),
                ));
            }
            page_navigation_handoffs = page_navigation_handoffs.saturating_add(1);
            let target_url =
                self.resolve_page_navigation_href(&content.url, &page_navigation.href)?;
            navigation = NativeNavigationRequest::get(target_url.clone());
            navigation.replace_history = page_navigation.replace_history;
            navigation.object_url = page_navigation.object_url;
            history_commit = if page_navigation.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            referrer = Some(content.url.clone());
            referrer_policy =
                NativeFetchReferrerPolicy::parse(&content.document.document_referrer_policy)?;
        }
    }

    pub(super) async fn terminate_content_process_after_navigation_cancel(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        if let Some(mut process) = self.content_process.take() {
            process
                .terminate_and_reap("cancel native navigation")
                .await?;
        }
        Ok(())
    }

    pub(super) fn apply_content_load_effects(
        &mut self,
        content: &mut NativeContentLoad,
    ) -> Result<(), NativeEngineError> {
        self.config.window_name = content.window_name.clone();
        self.publish_content_state(&content.storage_events, &content.indexed_db_changes)?;
        self.queue_popup_requests(std::mem::take(&mut content.popups))?;
        let content_origin = content.origin.clone();
        self.queue_post_message_requests_from_origin(
            std::mem::take(&mut content.post_messages),
            &content_origin,
        )?;
        self.queue_page_message_port_commands(std::mem::take(
            &mut content.page_message_port_commands,
        ))?;
        self.queue_shared_worker_create_commands(
            std::mem::take(&mut content.shared_worker_commands),
            content.origin.clone(),
            content.url.clone(),
        )?;
        self.queue_window_close_requests(std::mem::take(&mut content.window_closes))?;
        self.queue_window_navigation_requests(std::mem::take(&mut content.window_navigations))?;
        self.queue_service_worker_client_messages(std::mem::take(
            &mut content.service_worker_client_messages,
        ))?;
        self.queue_service_worker_open_window_requests(std::mem::take(
            &mut content.service_worker_open_windows,
        ))?;
        Ok(())
    }

    pub(super) async fn resume_pending_service_worker_navigation(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        let content_result = {
            let process =
                self.content_process
                    .as_mut()
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "service worker navigation resume".into(),
                        reason: "native content process is not running".into(),
                    })?;
            if !process.refresh_health() {
                return Err(NativeEngineError::worker_failure(
                    "service worker navigation resume",
                    process
                        .failure_kind()
                        .unwrap_or(NativeWorkerFailureKind::Exited),
                    "content process is unavailable after a failed service worker navigation",
                ));
            }
            self.request_ledger.begin()?;
            let result = Box::pin(process.resume_service_worker_navigation(&mut self.loader)).await;
            self.request_ledger.finish();
            result
        };
        self.reconcile_pending_content_cookie_changes("resumed navigation parent cookie changes")?;
        self.persist_pending_loader_cookie_changes()?;
        let content_result = content_result?;
        let mut content = match content_result {
            NativeContentLoadResult::Loaded(content) => *content,
            NativeContentLoadResult::Suspended(open_windows) => {
                self.queue_service_worker_open_window_requests(open_windows)?;
                return Ok(());
            }
        };
        let pending = self
            .pending_service_worker_navigation
            .take()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "service worker navigation resume".into(),
                reason: "content process resumed a navigation that the engine did not retain"
                    .into(),
            })?;
        self.apply_content_load_effects(&mut content)?;
        let policy_owner_url = self.url.clone();
        let policy_sources = self.document_navigate_to_sources.clone();
        if !self.allows_top_level_navigation_for_owner(
            &policy_owner_url,
            &content.url,
            policy_sources.as_deref(),
            false,
        )? {
            return Ok(());
        }
        if !self.is_same_document_navigation(&content.url)
            && !self.allows_frame_navigation_after_redirect(&pending.initial_url, &content.url)?
        {
            return Ok(());
        }
        self.commit_content_process().await?;
        let worker = self
            .runtime_worker
            .clone()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "service worker navigation resume".into(),
                reason: "native runtime worker is not running".into(),
            })?;
        self.navigate_content_async(
            content,
            &worker,
            pending.history_commit,
            pending.page_navigation_handoffs,
            &pending.initial_url,
        )
        .await
        .map(|_| ())
    }

    pub(super) async fn commit_content_process(&mut self) -> Result<(), NativeEngineError> {
        let Some(process) = self.content_process.as_mut() else {
            return Err(NativeEngineError::Worker {
                operation: "commit content process".into(),
                reason: "native content process is not running".into(),
            });
        };
        process.commit().await
    }

    pub(super) async fn navigate_resource_async(
        &mut self,
        resource: NativeResource,
        worker: &NativeRuntimeWorker,
        history_commit: HistoryCommit,
        page_navigation_handoffs: usize,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if is_network_url(&resource.url) {
            self.commit_content_process().await?;
        } else {
            self.content_process.take();
        }
        if self.is_same_document_navigation(&resource.url) {
            self.commit_same_document_navigation_async(
                resource.url,
                history_commit,
                worker,
                page_navigation_handoffs,
            )
            .await?;
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            if let Some(page_navigation) = self
                .commit_navigation_async(prepared, worker, history_commit)
                .await?
            {
                if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                    return Err(NativeEngineError::limit(
                        "page navigation handoffs",
                        MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                        page_navigation_handoffs.saturating_add(1),
                    ));
                }
                return Box::pin(
                    self.navigate_request_async(page_navigation, page_navigation_handoffs + 1),
                )
                .await;
            }
        }
        Ok(self.snapshot_unchecked())
    }

    pub(super) fn navigate_content(
        &mut self,
        content: NativeContentLoad,
        initial_url: &str,
        history_commit: HistoryCommit,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&content.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(content.url, history_commit)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation_after_redirect(initial_url, &content.url)? {
            return Ok(self.snapshot_unchecked());
        } else {
            let prepared = self.prepare_navigation_content(content)?;
            let _ = self.commit_navigation(prepared, history_commit)?;
        }
        Ok(self.snapshot_unchecked())
    }

    pub(super) async fn navigate_content_async(
        &mut self,
        content: NativeContentLoad,
        worker: &NativeRuntimeWorker,
        history_commit: HistoryCommit,
        page_navigation_handoffs: usize,
        initial_url: &str,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        let same_document = self.is_same_document_navigation(&content.url);
        if !same_document
            && !self.allows_frame_navigation_after_redirect(initial_url, &content.url)?
        {
            return Ok(self.snapshot_unchecked());
        }
        if same_document {
            self.commit_same_document_navigation_async(
                content.url,
                history_commit,
                worker,
                page_navigation_handoffs,
            )
            .await?;
        } else {
            let prepared = self.prepare_navigation_content(content)?;
            let _ = self
                .commit_navigation_async(prepared, worker, history_commit)
                .await?;
            if let Some(page_navigation) = self.dispatch_content_page_show_async().await? {
                if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                    return Err(NativeEngineError::limit(
                        "page navigation handoffs",
                        MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                        page_navigation_handoffs.saturating_add(1),
                    ));
                }
                return Box::pin(self.navigate_script_navigation_async(
                    page_navigation,
                    page_navigation_handoffs + 1,
                ))
                .await
                .map(|_| self.snapshot_unchecked());
            }
        }
        Ok(self.snapshot_unchecked())
    }

    pub(super) async fn dispatch_navigation_lifecycle_async(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let (canceled, sticky_activation, mut navigation) =
            self.dispatch_before_unload_for_navigation().await?;
        let allowed = if !canceled || !sticky_activation {
            true
        } else {
            self.confirm_beforeunload_for_navigation().await?
        };
        if !allowed {
            return Ok((false, None));
        }
        if let Some(next_navigation) = self.dispatch_unload_for_navigation().await? {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple outgoing lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        Ok((true, navigation))
    }

    pub async fn dispatch_before_unload_for_navigation(
        &mut self,
    ) -> Result<(bool, bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let (event_allowed, navigation) = if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            self.dispatch_content_before_unload_async().await?
        } else if self.javascript.is_some() {
            self.dispatch_local_before_unload()?
        } else {
            (true, None)
        };
        Ok((
            !event_allowed,
            self.document_has_sticky_activation,
            navigation,
        ))
    }

    pub async fn confirm_beforeunload_for_navigation(&self) -> Result<bool, NativeEngineError> {
        self.wait_for_beforeunload_confirmation_async().await
    }

    pub async fn dispatch_unload_for_navigation(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let events = [NativeEventKind::PageHide, NativeEventKind::Unload];
        let navigation = if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            self.dispatch_content_events_async(&events, true).await?
        } else if self.javascript.is_some() {
            self.dispatch_local_navigation_lifecycle()?
        } else {
            None
        };
        self.persist_local_web_storage()?;
        Ok(navigation)
    }

    pub(super) async fn wait_for_beforeunload_confirmation_async(
        &self,
    ) -> Result<bool, NativeEngineError> {
        let pending = self
            .dialog_control
            .wait_for_beforeunload(&self.config.context_id, &self.frame_id, &self.url)
            .await?;
        let accepted = pending.resolution.accepted;
        pending.finish();
        Ok(accepted)
    }

    pub(super) async fn dispatch_content_before_unload_async(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let mutation = {
            let Some(process) = self.content_process.as_mut() else {
                return Ok((true, None));
            };
            if !process.refresh_health() {
                return Ok((true, None));
            }
            process.dispatch_before_unload(&mut self.loader).await?
        };
        let navigation = mutation
            .navigation
            .clone()
            .map(|navigation| self.content_navigation_request(navigation))
            .transpose()?;
        let allowed = mutation.allowed;
        let next_revision = self.next_revision()?;
        self.apply_content_process_lifecycle_mutation_async_at(next_revision, mutation)
            .await?;
        Ok((allowed, navigation))
    }

    pub(super) async fn dispatch_content_events_async(
        &mut self,
        events: &[NativeEventKind],
        outgoing_lifecycle: bool,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let mutation = {
            let Some(process) = self.content_process.as_mut() else {
                return Ok(None);
            };
            if !process.refresh_health() {
                return Ok(None);
            }
            process
                .dispatch_lifecycle_events(events, &mut self.loader)
                .await?
        };
        let navigation = mutation
            .navigation
            .clone()
            .map(|navigation| self.content_navigation_request(navigation))
            .transpose()?;
        let next_revision = self.next_revision()?;
        if outgoing_lifecycle {
            self.apply_content_process_lifecycle_mutation_async_at(next_revision, mutation)
                .await?;
        } else {
            self.apply_content_process_mutation_async_at(next_revision, mutation)
                .await?;
        }
        Ok(navigation)
    }

    pub(super) async fn dispatch_content_page_show_async(
        &mut self,
    ) -> Result<Option<NativeContentNavigation>, NativeEngineError> {
        let Some(process) = self.content_process.as_mut() else {
            return Ok(None);
        };
        if !process.refresh_health() {
            return Ok(None);
        }
        let mutation = process
            .dispatch_lifecycle_events(&[NativeEventKind::PageShow], &mut self.loader)
            .await?;
        let navigation = mutation.navigation.clone();
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        Ok(navigation)
    }

    pub(super) async fn dispatch_content_hash_change_async(
        &mut self,
        old_url: &str,
        new_url: &str,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let mutation = {
            let Some(process) = self.content_process.as_mut() else {
                return Ok(None);
            };
            if !process.refresh_health() {
                return Ok(None);
            }
            process
                .dispatch_hash_change(old_url, new_url, &mut self.loader)
                .await?
        };
        let navigation = mutation
            .navigation
            .clone()
            .map(|navigation| self.content_navigation_request(navigation))
            .transpose()?;
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        Ok(navigation)
    }

    /// Move to the previous bounded local history entry.
    ///
    /// `None` is an explicit boundary no-op and leaves the engine unchanged.
    pub fn go_back(&mut self) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history(NativeHistoryDirection::Back, "go back")
    }

    /// Move to the next bounded local history entry.
    ///
    /// `None` is an explicit boundary no-op and leaves the engine unchanged.
    pub fn go_forward(&mut self) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history(NativeHistoryDirection::Forward, "go forward")
    }

    /// Move to the previous history entry through the asynchronous native
    /// navigation owner. Network entries are loaded by the sandboxed content
    /// process; local entries use the deterministic loader.
    pub async fn go_back_async(
        &mut self,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history_async(NativeHistoryDirection::Back, "go back", true)
            .await
    }

    /// Move to the next history entry through the asynchronous native
    /// navigation owner. A missing forward entry is an explicit boundary
    /// no-op.
    pub async fn go_forward_async(
        &mut self,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history_async(NativeHistoryDirection::Forward, "go forward", true)
            .await
    }

    pub fn history_target_is_cross_document(
        &self,
        direction: NativeHistoryDirection,
    ) -> Option<bool> {
        let history_index = self.history.target_index(direction)?;
        let target_url = &self.history.entry(history_index)?.url;
        Some(
            !self.history.is_same_document(history_index)
                && !self.is_same_document_navigation(target_url),
        )
    }

    pub async fn traverse_history_async_after_lifecycle(
        &mut self,
        direction: NativeHistoryDirection,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        let operation = match direction {
            NativeHistoryDirection::Back => "go back",
            NativeHistoryDirection::Forward => "go forward",
        };
        self.traverse_history_async(direction, operation, false)
            .await
    }

    pub fn snapshot(&self) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("evidence")?;
        Ok(self.snapshot_unchecked())
    }

    /// Execute one bounded native GET/fetch request from the current external
    /// document through the parent-owned resource loader. This is deliberately
    /// narrower than the JavaScript Fetch/Web IDL surface and accepts no custom
    /// headers or body.
    pub async fn fetch_async(
        &mut self,
        href: impl Into<String>,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        self.require_running("fetch")?;
        if !is_network_url(&self.url) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native fetch requires an HTTP(S) document".into(),
            });
        }
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let href = href.into();
        self.ensure_content_process().await?;
        self.reconcile_pending_content_cookie_changes("parent Fetch cookie snapshot")?;
        self.request_ledger.begin()?;
        let result = match self.content_process.as_mut() {
            Some(process) => {
                process
                    .fetch(&self.url, &href, credentials, &mut self.loader)
                    .await
            }
            None => Err(NativeEngineError::Worker {
                operation: "content process fetch".into(),
                reason: "native content process is not running".into(),
            }),
        };
        let cookie_changes = self.loader.take_cookie_changes();
        let result = if cookie_changes.is_empty() {
            result
        } else {
            let sync_result = async {
                self.publish_external_cookie_changes(&cookie_changes)?;
                let document_url = self.url.clone();
                let document_cookie = self.loader.document_cookie(&document_url)?;
                if let Some(process) = self.content_process.as_mut() {
                    process
                        .sync_document_cookie_projection(&document_url, &document_cookie)
                        .await?;
                }
                Ok(())
            }
            .await;
            match (result, sync_result) {
                (Ok(response), Ok(())) => Ok(response),
                (Err(error), Ok(())) => Err(error),
                (_, Err(error)) => Err(error),
            }
        };
        self.request_ledger.finish();
        result
    }

    /// Evaluate bounded ECMAScript in the current page realm. Network
    /// documents execute in the child-owned content process; local documents
    /// use the same runtime implementation in the engine owner.
    pub async fn evaluate_async(
        &mut self,
        source: impl Into<String>,
    ) -> Result<serde_json::Value, NativeEngineError> {
        self.evaluate_page_with_events_async(source.into(), NativePageEventBatch::default())
            .await
    }
}
