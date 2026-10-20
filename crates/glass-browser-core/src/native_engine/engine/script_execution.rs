use super::*;

impl NativeEngine {
    pub(super) async fn pump_local_worker_fetch_streams(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        for _ in 0..MAX_NATIVE_WORKER_MESSAGES {
            tokio::task::yield_now().await;
            if !self
                .workers
                .pump_fetch_stream_event(&mut self.loader)
                .await?
            {
                break;
            }
        }
        Ok(())
    }

    pub(super) async fn evaluate_page_with_events_async(
        &mut self,
        source: String,
        page_events: NativePageEventBatch,
    ) -> Result<serde_json::Value, NativeEngineError> {
        self.evaluate_page_with_events_async_timeout(
            source,
            page_events,
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
        )
        .await
    }

    pub(super) async fn evaluate_page_with_events_async_timeout(
        &mut self,
        source: String,
        mut page_events: NativePageEventBatch,
        content_process_timeout: Duration,
    ) -> Result<serde_json::Value, NativeEngineError> {
        self.require_running("script")?;
        self.sync_external_storage_events()?;
        if self.content_process.is_some() {
            self.deliver_pending_external_storage_events().await?;
            self.reconcile_pending_content_cookie_changes(
                "page-script parent Fetch cookie snapshot",
            )?;
            if let Some(process) = self.content_process.as_mut() {
                process
                    .sync_frame_script_context(self.frame_script_context.as_ref())
                    .await?;
                process
                    .sync_frame_script_bindings(&self.frame_script_bindings)
                    .await?;
                process.sync_scroll_offset(self.scroll_offset).await?;
                process
                    .sync_nested_scroll_offsets(&self.nested_scroll_offsets)
                    .await?;
            }
            let process_result = {
                let process = self
                    .content_process
                    .as_mut()
                    .expect("content process presence was checked");
                if !process.refresh_health() {
                    return Err(NativeEngineError::worker_failure(
                        "content process script",
                        process
                            .failure_kind()
                            .unwrap_or(NativeWorkerFailureKind::Exited),
                        "content process is unavailable after a failed operation; navigate to recover it",
                    ));
                }
                self.request_ledger.begin()?;
                let result = process
                    .evaluate_with_page_events_timeout(
                        &source,
                        &page_events,
                        Some(&mut self.loader),
                        content_process_timeout,
                    )
                    .await;
                self.request_ledger.finish();
                result
            };
            self.reconcile_pending_content_cookie_changes(
                "page-script parent document.cookie writes",
            )?;
            let cookie_changes = self.loader.take_cookie_changes();
            let cookie_persist_result = if cookie_changes.is_empty() {
                Ok(())
            } else if let Err(error) = self.publish_external_cookie_changes(&cookie_changes) {
                self.loader.restore_cookie_changes(cookie_changes);
                Err(error)
            } else {
                Ok(())
            };
            let script_result = match (process_result, cookie_persist_result) {
                (Ok(result), Ok(())) => result,
                (Err(error), Ok(())) => return Err(error),
                (_, Err(error)) => return Err(error),
            };
            let NativeContentScriptResult {
                value,
                mutation,
                storage_events,
                indexed_db_changes,
                dialogs,
                popups,
                post_messages,
                page_message_port_commands,
                shared_worker_commands,
                window_closes,
                window_navigations,
                service_worker_client_messages,
                service_worker_open_windows,
                service_worker_fetch_resumed: _,
                frame_scripts,
                window_name,
                mut history,
            } = script_result;
            self.config.window_name = window_name;
            self.queue_frame_script_requests(frame_scripts)?;
            self.queue_page_message_port_commands(page_message_port_commands)?;
            self.queue_shared_worker_create_commands(
                shared_worker_commands,
                self.origin.clone(),
                self.url.clone(),
            )?;
            self.queue_service_worker_client_messages(service_worker_client_messages)?;
            self.queue_service_worker_open_window_requests(service_worker_open_windows)?;
            let mut history_traversal = None;
            let mutation_history = mutation
                .as_ref()
                .map(|mutation| mutation.history.clone())
                .unwrap_or_default();
            history.extend(mutation_history);
            if let Some(mut mutation) = mutation {
                let csp_violations = std::mem::take(&mut mutation.csp_violations);
                let navigation = mutation.navigation.clone();
                self.apply_content_process_mutation(mutation)?;
                self.dispatch_content_csp_violations_async(csp_violations)
                    .await?;
                history_traversal = self.apply_content_history_commands(&history)?;
                if let Some(navigation) = navigation {
                    self.navigate_script_navigation_async(navigation, 0).await?;
                }
            } else {
                self.publish_content_state(&storage_events, &indexed_db_changes)?;
                self.queue_popup_requests(popups)?;
                self.queue_post_message_requests(post_messages)?;
                self.queue_window_close_requests(window_closes)?;
                self.queue_window_navigation_requests(window_navigations)?;
                let dialog_url = self.url.clone();
                self.install_dialogs(dialogs, &dialog_url)?;
            }
            if !history.is_empty() {
                self.sync_content_history_async().await?;
            }
            if let Some(delta) = history_traversal
                && delta != 0
            {
                self.traverse_history_delta_async(delta).await?;
            }
            return Ok(value);
        }
        if self.javascript.is_none() {
            let javascript = NativeJavaScriptRuntime::new_with_context_metadata(
                &self.config.context_id,
                &self.config.window_name,
                self.config.opener_context_id.as_deref(),
                &self.config.opener_window_name,
                &self.config.opener_url,
            )?;
            javascript.set_storage_state(self.web_storage.clone());
            javascript.set_indexed_db_state(
                self.indexed_db
                    .origin(&storage_key(&self.url, &self.origin)),
            );
            javascript.set_frame_script_bindings(self.frame_script_bindings.clone());
            javascript.set_frame_script_context(self.frame_script_context.clone());
            javascript.set_frame_id(self.frame_id.clone());
            javascript.set_environment(self.environment.clone());
            javascript.set_scroll_offset(self.scroll_offset);
            javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
            self.javascript = Some(javascript);
        }
        self.deliver_pending_external_storage_events().await?;
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_environment(self.environment.clone());
        }
        let cookie = self.loader.document_cookie(&self.url)?;
        self.javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .set_cookie_state(cookie);
        self.sync_javascript_scroll_offset();
        self.sync_javascript_history();
        let initial_worker_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_worker_commands();
        let owner_url = self.url.clone();
        self.workers
            .apply_commands(initial_worker_commands, &mut self.loader, &owner_url)
            .await?;
        let initial_message_port_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_message_port_commands();
        self.workers
            .apply_page_message_port_commands(initial_message_port_commands, &mut self.loader)
            .await?;
        self.collect_page_message_port_commands()?;
        self.pump_local_worker_fetch_streams().await?;
        self.workers.run_due_timers(&mut self.loader).await?;
        self.pump_local_worker_fetch_streams().await?;
        page_events
            .worker_messages
            .extend(self.workers.take_messages());
        page_events
            .message_port_messages
            .extend(std::mem::take(&mut self.pending_message_port_messages));
        page_events
            .message_port_messages
            .extend(self.workers.take_message_port_messages());
        let evaluation = {
            let javascript = self
                .javascript
                .as_ref()
                .expect("local JavaScript runtime initialized");
            self.synchronize_local_inline_csp_policy(&self.document, javascript, &self.url)?;
            javascript.set_sync_xhr_loader(&self.loader);
            let result = javascript.evaluate_with_page_events(
                &source,
                &self.document,
                &self.url,
                &self.origin,
                self.config.viewport,
                &page_events,
            );
            if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            result?
        };
        let worker_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_worker_commands();
        let owner_url = self.url.clone();
        self.workers
            .apply_commands(worker_commands, &mut self.loader, &owner_url)
            .await?;
        let message_port_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_message_port_commands();
        self.workers
            .apply_page_message_port_commands(message_port_commands, &mut self.loader)
            .await?;
        self.collect_page_message_port_commands()?;
        self.pump_local_worker_fetch_streams().await?;
        self.pending_message_port_messages
            .extend(self.workers.take_message_port_messages());
        if !self.workers.take_websocket_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker WebSocket requires a process-backed HTTP(S) document".into(),
            });
        }
        if !self.workers.take_event_source_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker EventSource requires a process-backed HTTP(S) document".into(),
            });
        }
        if evaluation.top_level_await_pending {
            return Err(NativeEngineError::Worker {
                operation: "evaluate JavaScript".into(),
                reason: "top-level await remained pending without a native host operation".into(),
            });
        }
        self.drain_local_popups()?;
        let navigation = self.apply_local_script_commands(&evaluation.commands, true)?;
        let dynamic_worker_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_worker_commands();
        let owner_url = self.url.clone();
        self.workers
            .apply_commands(dynamic_worker_commands, &mut self.loader, &owner_url)
            .await?;
        let dynamic_message_port_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_message_port_commands();
        self.workers
            .apply_page_message_port_commands(dynamic_message_port_commands, &mut self.loader)
            .await?;
        self.collect_page_message_port_commands()?;
        self.pump_local_worker_fetch_streams().await?;
        self.pending_message_port_messages
            .extend(self.workers.take_message_port_messages());
        if !self.workers.take_websocket_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker WebSocket requires a process-backed HTTP(S) document".into(),
            });
        }
        if !self.workers.take_event_source_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker EventSource requires a process-backed HTTP(S) document".into(),
            });
        }
        let frame_scripts = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_frame_script_events();
        self.queue_frame_script_requests(frame_scripts)?;
        self.drain_local_dialogs()?;
        self.persist_local_web_storage()?;
        if let Some(navigation) = navigation {
            self.navigate_request_async(navigation, 0).await?;
        }
        Ok(evaluation.value)
    }

    pub(super) async fn dispatch_content_csp_violations_async(
        &mut self,
        csp_violations: Vec<NativeCspViolation>,
    ) -> Result<(), NativeEngineError> {
        if csp_violations.is_empty() {
            return Ok(());
        }
        let page_events = NativePageEventBatch {
            csp_violations,
            ..NativePageEventBatch::default()
        };
        Box::pin(self.evaluate_page_with_events_async("undefined;".into(), page_events)).await?;
        Ok(())
    }

    pub async fn sync_window_proxies(
        &mut self,
        updates: &[NativeWindowProxyUpdate],
    ) -> Result<(), NativeEngineError> {
        self.require_running("WindowProxy synchronization")?;
        if updates.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native WindowProxy updates",
                MAX_NATIVE_EFFECTS,
                updates.len(),
            ));
        }
        if let Some(process) = self.content_process.as_mut() {
            process.sync_window_proxies(updates).await
        } else if let Some(javascript) = self.javascript.as_ref() {
            javascript.sync_window_proxies(updates)
        } else {
            Ok(())
        }
    }

    /// Read or mutate the page-visible Web Storage owned by this engine.
    ///
    /// Semantic backend storage must use the same origin-keyed state as the
    /// JavaScript realm. Keeping a second adapter-owned map would make a
    /// successful `storage` operation invisible to page script (and vice
    /// versa), which is a correctness failure for a browser backend.
    pub fn pending_dialog(&self) -> Result<Option<PendingDialog>, NativeEngineError> {
        self.require_running("inspect dialog")?;
        Ok(self.pending_dialogs.front().cloned())
    }

    /// Report whether the native request owner has remained idle for the
    /// caller's bounded quiet interval.
    pub fn network_quiet(&self, duration: Duration) -> Result<(bool, String), NativeEngineError> {
        self.require_running("inspect network activity")?;
        Ok(self.request_ledger.quiet(duration))
    }

    /// Return the bounded native download IDs visible to the browser backend.
    pub fn download_ids(&self) -> Result<Vec<String>, NativeEngineError> {
        self.require_running("list downloads")?;
        Ok(self
            .pending_downloads
            .iter()
            .map(|download| download.guid.clone())
            .chain(self.completed_download_ids.iter().cloned())
            .collect())
    }

    /// Cancel one queued native download before its bytes are fetched.
    pub fn cancel_download(&mut self, download_id: &str) -> Result<bool, NativeEngineError> {
        self.require_running("cancel download")?;
        if download_id.is_empty() || download_id.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::invalid(
                "download id",
                "must be a non-empty bounded value",
            ));
        }
        let before = self.pending_downloads.len();
        self.pending_downloads
            .retain(|download| download.guid != download_id);
        Ok(self.pending_downloads.len() != before)
    }

    /// Complete the oldest queued native download into an already-authorized
    /// directory. The click action only queues the request, which keeps the
    /// action boundary responsive and lets the caller choose its destination.
    pub async fn wait_for_download_async(
        &mut self,
        destination: &Path,
        deadline: Duration,
    ) -> Result<DownloadOutcome, NativeEngineError> {
        self.require_running("download")?;
        if deadline.is_zero() || deadline > MAX_NATIVE_DOWNLOAD_DEADLINE {
            return Err(NativeEngineError::invalid(
                "download deadline",
                "must be between 1 ms and 30 seconds",
            ));
        }
        if !std::fs::metadata(destination).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(NativeEngineError::invalid(
                "download destination",
                "must be an existing directory",
            ));
        }
        let Some(pending) = self.pending_downloads.pop_front() else {
            return Err(NativeEngineError::Network {
                operation: "download".into(),
                reason: "no native download is pending; activate a download link first".into(),
            });
        };
        self.request_ledger.begin()?;
        let response = match tokio::time::timeout(
            deadline,
            self.loader.download_async(&self.url, &pending.url),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                self.pending_downloads.push_front(pending);
                self.request_ledger.finish();
                return Err(error);
            }
            Err(_) => {
                self.pending_downloads.push_front(pending);
                self.request_ledger.finish();
                return Err(NativeEngineError::Network {
                    operation: "download".into(),
                    reason: "download exceeded its deadline".into(),
                });
            }
        };
        self.request_ledger.finish();
        if !(200..=299).contains(&response.status) {
            self.pending_downloads.push_front(pending);
            return Err(NativeEngineError::Network {
                operation: "download".into(),
                reason: format!("server returned HTTP {}", response.status),
            });
        }
        match write_native_download(destination, &pending.suggested_filename, &response.body).await
        {
            Ok(_) => {}
            Err(error) => {
                self.pending_downloads.push_front(pending);
                return Err(error);
            }
        };
        let mut digest = Sha256::new();
        digest.update(&response.body);
        let sha256 = format!("{:x}", digest.finalize());
        self.completed_downloads = self.completed_downloads.saturating_add(1);
        self.completed_download_ids.push_back(pending.guid.clone());
        while self.completed_download_ids.len() > MAX_NATIVE_COMPLETED_DOWNLOAD_IDS {
            self.completed_download_ids.pop_front();
        }
        Ok(DownloadOutcome {
            guid: pending.guid,
            suggested_filename: pending.suggested_filename,
            state: "completed".into(),
            received_bytes: response.body.len() as u64,
            total_bytes: response.body.len() as u64,
            target_id: pending.target_id,
            frame_id: pending.frame_id,
            sha256: Some(sha256),
        })
    }

    /// Number of native downloads that completed in this engine instance.
    pub fn completed_download_count(&self) -> Result<u64, NativeEngineError> {
        self.require_running("inspect downloads")?;
        Ok(self.completed_downloads)
    }

    /// Resolve one queued JavaScript dialog without creating a browser or
    /// routing through CDP. The page realm's bounded alert/confirm/prompt
    /// result is deterministic for the current script batch; resolution
    /// removes the user-facing prompt and lets the next queued prompt surface.
    pub fn resolve_dialog(
        &mut self,
        _decision: PromptDecision,
    ) -> Result<PromptResult, NativeEngineError> {
        self.require_running("resolve dialog")?;
        Ok(PromptResult {
            handled: self.pending_dialogs.pop_front().is_some(),
        })
    }
}
