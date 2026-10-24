use super::*;

/// Retire the worker if an in-flight IPC future is dropped by its caller.
/// The worker may be waiting for a parent navigation response, so the next
/// IPC request cannot safely share that protocol exchange.
struct KillContentChildOnDrop {
    pid: Option<u32>,
    aborted: Arc<AtomicBool>,
    armed: bool,
}

impl KillContentChildOnDrop {
    fn new(pid: Option<u32>, aborted: Arc<AtomicBool>) -> Self {
        Self {
            pid,
            aborted,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for KillContentChildOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.aborted.store(true, Ordering::Release);
        #[cfg(unix)]
        if let Some(pid) = self.pid.and_then(|pid| libc::pid_t::try_from(pid).ok()) {
            // SAFETY: this is the unreaped child owned by this content process;
            // its active IPC future is being aborted, so its protocol is stale.
            let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

fn decode_content_process_error_response(
    response: &Value,
    operation: &str,
    fallback_reason: &str,
) -> NativeEngineError {
    if response.get("error_type").and_then(Value::as_str) == Some("resource_limit")
        && let (Some(resource), Some(limit), Some(actual)) = (
            response.get("resource").and_then(Value::as_str),
            response
                .get("limit")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok()),
            response
                .get("actual")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok()),
        )
    {
        return NativeEngineError::limit(resource, limit, actual);
    }
    NativeEngineError::Worker {
        operation: operation.into(),
        reason: response
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or(fallback_reason)
            .into(),
    }
}

#[cfg(test)]
mod content_error_response_tests {
    use super::*;

    #[test]
    fn resource_limit_errors_keep_their_typed_identity_across_content_ipc() {
        let response = json!({
            "kind": "error",
            "error_type": "resource_limit",
            "resource": "CSS style rules",
            "limit": 4_096,
            "actual": 4_097,
            "reason": "CSS style rules exceeds limit 4096 (actual 4097)",
        });

        assert_eq!(
            decode_content_process_error_response(&response, "content process load", "fallback",),
            NativeEngineError::limit("CSS style rules", 4_096, 4_097)
        );
    }

    #[test]
    fn malformed_or_untyped_content_errors_remain_worker_errors() {
        let response = json!({
            "kind": "error",
            "reason": "content process rejected the document load",
        });

        assert_eq!(
            decode_content_process_error_response(&response, "content process load", "fallback",),
            NativeEngineError::Worker {
                operation: "content process load".into(),
                reason: "content process rejected the document load".into(),
            }
        );
    }
}

#[path = "process/exchange.rs"]
mod exchange;
#[path = "process/parent_streams.rs"]
mod parent_streams;

impl NativeContentProcess {
    #[cfg(test)]
    pub(crate) async fn spawn(
        storage_path: Option<&Path>,
        allowed_file_roots: &[PathBuf],
        dialog_control: NativeDialogControlPlane,
        pending_cookie_changes: Arc<Mutex<Vec<NativeCookieChange>>>,
    ) -> Result<Self, NativeEngineError> {
        Self::spawn_with_event_notify(
            storage_path,
            allowed_file_roots,
            dialog_control,
            pending_cookie_changes,
            None,
        )
        .await
    }

    pub(crate) async fn spawn_with_event_notify(
        storage_path: Option<&Path>,
        allowed_file_roots: &[PathBuf],
        dialog_control: NativeDialogControlPlane,
        pending_cookie_changes: Arc<Mutex<Vec<NativeCookieChange>>>,
        event_notify: Option<Arc<tokio::sync::Notify>>,
    ) -> Result<Self, NativeEngineError> {
        migrate_cookie_profile_from_web_storage(storage_path)?;
        let path = worker_binary_path()?;
        if let Some(storage_path) = storage_path
            && !storage_path.exists()
        {
            save_web_storage_profile(
                Some(storage_path),
                &NativeWebStorageState::default(),
                &[],
                &[],
                &[],
                &NativeIndexedDbState::default(),
                &[],
            )?;
        }
        let (mut command, mut sandbox) =
            prepare_worker_command(&path, storage_path, allowed_file_roots)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if std::env::var_os("RUST_BACKTRACE").is_some() {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "spawn content process",
                    NativeWorkerFailureKind::Spawn,
                    "native content worker could not be started",
                )
            })?;
        if let Err(error) = sandbox.attach(&child) {
            let _ = child.start_kill();
            return Err(error);
        }
        let stdin = child.stdin.take().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "spawn content process",
                NativeWorkerFailureKind::Transport,
                "native content worker stdin is unavailable",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "spawn content process",
                NativeWorkerFailureKind::Transport,
                "native content worker stdout is unavailable",
            )
        })?;
        let (stdout_sender, stdout_frames) = mpsc::channel(MAX_CONTENT_PROCESS_OUTPUT_FRAMES);
        let stdout_notify = event_notify.clone();
        let stdout_reader = tokio::spawn(async move {
            forward_content_process_output(stdout, stdout_sender, stdout_notify).await;
        });
        let mut process = Self {
            child,
            stdin,
            stdout_frames,
            stdout_reader,
            next_request_id: 1,
            next_async_event_sequence: 1,
            pending_async_effect_notifications: VecDeque::new(),
            healthy: true,
            ipc_aborted: Arc::new(AtomicBool::new(false)),
            failure_kind: None,
            scroll_offset: NativePoint { x: 0, y: 0 },
            nested_scroll_offsets: BTreeMap::new(),
            current_document_url: None,
            current_document_generation: None,
            pending_navigation_request: None,
            parent_brokered_script_owner: None,
            parent_brokered_script_urls: BTreeSet::new(),
            parent_brokered_policy_owner: None,
            parent_brokered_meta_policies: BTreeSet::new(),
            context_id: None,
            frame_id: None,
            dialog_control,
            pending_cookie_changes,
            pending_cookie_writes: Vec::new(),
            next_parent_event_source_stream_id: 1,
            parent_event_source_streams: BTreeMap::new(),
            next_parent_websocket_stream_id: 1,
            parent_websocket_streams: BTreeMap::new(),
            #[cfg(windows)]
            sandbox,
        };
        let id = process.next_id();
        let response = process
            .exchange(json!({
                "kind": "ping",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        require_response_kind(&response, "pong", id, "content process ping")?;
        if response.get("protocol").and_then(Value::as_u64) != Some(CONTENT_WORKER_PROTOCOL_VERSION)
        {
            return Err(NativeEngineError::worker_failure(
                "content process ping",
                NativeWorkerFailureKind::Protocol,
                "content process protocol version is unsupported",
            ));
        }
        Ok(process)
    }

    pub(crate) fn has_current_document(&self) -> bool {
        self.current_document_url.is_some()
    }

    pub(crate) const fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub(crate) async fn start(
        &mut self,
        storage_path: Option<&Path>,
        allowed_file_roots: &[PathBuf],
        context_id: &str,
        frame_id: &str,
        window_name: &str,
        opener_context_id: Option<&str>,
        opener_window_name: &str,
        opener_url: &str,
        frame_context: Option<&NativeFrameScriptContext>,
        environment: &NativeEnvironmentOverrides,
        service_worker_clients: &[NativeServiceWorkerClientState],
        external_shared_worker_routing: bool,
    ) -> Result<(), NativeEngineError> {
        environment.validate()?;
        let id = self.next_id();
        self.context_id = Some(context_id.to_owned());
        self.frame_id = Some(frame_id.to_owned());
        let response = self
            .exchange(json!({
                "kind": "start",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "storage_path": storage_path.map(|path| path.to_string_lossy().into_owned()),
                "allowed_file_roots": allowed_file_roots
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                "context_id": context_id,
                "frame_id": frame_id,
                "window_name": window_name,
                "opener_context_id": opener_context_id,
                "opener_window_name": opener_window_name,
                "opener_url": opener_url,
                "frame_context": frame_context,
                "environment": environment,
                "service_worker_clients": service_worker_clients,
                "modal_dialogs": self.dialog_control.modal_dialogs_enabled(),
                "external_shared_worker_routing": external_shared_worker_routing,
            }))
            .await?;
        let result = require_response_kind(&response, "started", id, "content process start");
        if result.is_ok() {
            self.scroll_offset = NativePoint { x: 0, y: 0 };
            self.nested_scroll_offsets.clear();
        }
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn set_environment(
        &mut self,
        environment: &NativeEnvironmentOverrides,
    ) -> Result<(), NativeEngineError> {
        environment.validate()?;
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "environment_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "environment": environment,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process environment synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process environment synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "environment_synced",
            id,
            "content process environment synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            // A timed-out load can leave the worker consuming CPU in parsing or
            // script execution. Do not leave it behind for browser.stop to
            // discover later: stopping must not wait on a worker whose request
            // deadline already expired.
            let _ = self
                .terminate_and_reap("stop failed content process load")
                .await;
        }
        result
    }

    pub(crate) async fn set_viewport(
        &mut self,
        viewport: Viewport,
    ) -> Result<(), NativeEngineError> {
        viewport.validate()?;
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "viewport_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "width": viewport.width,
                "height": viewport.height,
                "device_scale_factor_milli": viewport.device_scale_factor_milli,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process viewport synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process viewport synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "viewport_synced",
            id,
            "content process viewport synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    /// Replace this worker's document snapshot after a browser-coordinated
    /// cross-context DOM transfer. The generation check makes a stale worker
    /// reject the complete candidate before changing its persistent document.
    pub(crate) async fn sync_document(
        &mut self,
        document: &NativeDocumentWire,
        limits: &NativeEngineLimits,
        generation: u32,
    ) -> Result<(), NativeEngineError> {
        limits.validate()?;
        let encoded = serde_json::to_vec(document).map_err(|_| NativeEngineError::Worker {
            operation: "content process document synchronization".into(),
            reason: "document snapshot could not be encoded".into(),
        })?;
        let maximum = MAX_CONTENT_IPC_FRAME_BYTES.saturating_sub(4096);
        if encoded.len() > maximum {
            return Err(NativeEngineError::limit(
                "content-process document synchronization",
                maximum,
                encoded.len(),
            ));
        }
        let id = self.next_id();
        let response = self
            .exchange_with_timeout(
                json!({
                    "kind": "document_sync",
                    "id": id,
                    "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                    "generation": generation,
                    "limits": limits,
                    "document": document,
                }),
                "content process document synchronization",
                CONTENT_PROCESS_SCRIPT_TIMEOUT,
            )
            .await?;
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process document synchronization".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the document snapshot")
                    .into(),
            });
        }
        let result = require_response_kind(
            &response,
            "document_synced",
            id,
            "content process document synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        if result.is_ok() {
            self.current_document_generation = Some(generation);
        }
        result
    }

    pub(crate) async fn sync_service_worker_clients(
        &mut self,
        clients: &[NativeServiceWorkerClientState],
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "service_worker_clients_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "clients": clients,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process Service Worker client synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process Service Worker client synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "service_worker_clients_synced",
            id,
            "content process Service Worker client synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_service_worker_registrations(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "service_worker_registrations_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content-process Service Worker registration synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content-process Service Worker registration synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "service_worker_registrations_synced",
            id,
            "content-process Service Worker registration synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn commit(&mut self) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "commit",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        let result = require_response_kind(&response, "committed", id, "content process commit");
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_frame_script_context(
        &mut self,
        context: Option<&NativeFrameScriptContext>,
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "frame_script_context_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "context": context,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process frame script context synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process frame script context synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "frame_script_context_synced",
            id,
            "content process frame script context synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn load(
        &mut self,
        navigation: &NativeNavigationRequest,
        limits: &NativeEngineLimits,
        viewport: Viewport,
        referrer: Option<&str>,
        referrer_policy: NativeFetchReferrerPolicy,
        client_id: &str,
        service_worker_clients: &[NativeServiceWorkerClientState],
        cancellation: Option<&NativeNavigationCancellation>,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentLoadResult, NativeEngineError> {
        self.load_with_deadline(
            navigation,
            limits,
            viewport,
            referrer,
            referrer_policy,
            client_id,
            service_worker_clients,
            cancellation,
            parent_loader,
            CONTENT_PROCESS_LOAD_TIMEOUT,
        )
        .await
    }

    pub(super) async fn load_with_deadline(
        &mut self,
        navigation: &NativeNavigationRequest,
        limits: &NativeEngineLimits,
        viewport: Viewport,
        referrer: Option<&str>,
        referrer_policy: NativeFetchReferrerPolicy,
        client_id: &str,
        service_worker_clients: &[NativeServiceWorkerClientState],
        cancellation: Option<&NativeNavigationCancellation>,
        parent_loader: &mut NativeResourceLoader,
        deadline: Duration,
    ) -> Result<NativeContentLoadResult, NativeEngineError> {
        let generation = self
            .current_document_generation
            .unwrap_or_default()
            .checked_add(1)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "content document generations",
                    u32::MAX as usize,
                    usize::MAX,
                )
            })?;
        let id = self.next_id();
        let request = json!({
            "kind": "load",
            "id": id,
            "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            "generation": generation,
            "url": navigation.url,
            "method": navigation.method.as_str(),
            "body": navigation.body.as_ref().and_then(|body| match body {
                NativeRequestBody::Text(body) => Some(body),
                NativeRequestBody::Bytes(_) | NativeRequestBody::StreamedBytes(_) => None,
            }),
            "body_base64": navigation.body.as_ref().and_then(|body| match body {
                NativeRequestBody::Text(_) => None,
                NativeRequestBody::Bytes(body) | NativeRequestBody::StreamedBytes(body) => Some(
                    base64::engine::general_purpose::STANDARD.encode(body),
                ),
            }),
            "content_type": navigation.body_content_type,
            "object_url": navigation.object_url,
            "referrer": referrer,
            "referrer_policy": referrer_policy.as_str(),
            "max_document_bytes": limits.max_document_bytes,
            "max_nodes": limits.max_nodes,
            "max_dom_depth": limits.max_dom_depth,
            "max_text_bytes": limits.max_text_bytes,
            "viewport_width": viewport.width,
            "viewport_height": viewport.height,
            "viewport_device_scale_factor_milli": viewport.device_scale_factor_milli,
            "client_id": client_id,
            "service_worker_clients": service_worker_clients,
        });
        let parent_request = request.clone();
        if cancellation.is_some_and(NativeNavigationCancellation::is_cancelled) {
            return Err(NativeEngineError::NavigationCancelled);
        }
        let response_result = if let Some(cancellation) = cancellation {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => None,
                response = self.exchange_with_parent_loader_timeout(
                    request,
                    "content process load",
                    deadline,
                    Some(parent_loader),
                ) => {
                    Some(response)
                }
            }
        } else {
            Some(
                self.exchange_with_parent_loader_timeout(
                    request,
                    "content process load",
                    deadline,
                    Some(parent_loader),
                )
                .await,
            )
        };
        let response = match response_result {
            Some(Ok(response)) => response,
            Some(Err(error)) => {
                self.terminate_and_reap("stop failed content process load")
                    .await?;
                return Err(error);
            }
            None => {
                self.terminate_and_reap("cancel content process load")
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            let error = decode_content_process_error_response(
                &response,
                "content process load",
                "content process rejected the document load",
            );
            // A worker that has never committed a document cannot service
            // document-owned requests after a rejected initial load. Retire
            // it now so the next navigation starts with a clean generation
            // owner. Once a document has committed, leave that process and
            // its revision intact when a later navigation fails.
            if self.current_document_generation.is_none() {
                self.terminate_and_reap("retire content process after failed initial load")
                    .await?;
            }
            return Err(error);
        }
        let result = decode_load_response(&response, id);
        if cancellation.is_some_and(NativeNavigationCancellation::is_cancelled) {
            self.terminate_and_reap("cancel content process load")
                .await?;
            return Err(NativeEngineError::NavigationCancelled);
        }
        if let Ok(NativeContentLoadResult::Loaded(content)) = &result {
            self.discard_stale_parent_event_source_streams(content.generation);
            self.current_document_url = Some(content.url.clone());
            self.current_document_generation = Some(content.generation);
            self.pending_navigation_request = None;
        } else if matches!(&result, Ok(NativeContentLoadResult::Suspended(_))) {
            self.pending_navigation_request = Some(parent_request);
        }
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn resume_service_worker_navigation(
        &mut self,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentLoadResult, NativeEngineError> {
        let id = self.next_id();
        let mut request = self.pending_navigation_request.clone().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "content process service worker fetch resume",
                NativeWorkerFailureKind::Protocol,
                "the parent has no captured navigation request to resume",
            )
        })?;
        request["id"] = Value::from(id);
        request["protocol"] = Value::from(CONTENT_WORKER_PROTOCOL_VERSION);
        request["resume_service_worker_fetch"] = Value::Bool(true);
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange_with_parent_loader_timeout(
                request,
                "content process service worker fetch resume",
                CONTENT_PROCESS_LOAD_TIMEOUT,
                Some(parent_loader),
            ),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process service worker fetch resume",
                    NativeWorkerFailureKind::Timeout,
                    "content process service worker fetch resume exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(decode_content_process_error_response(
                &response,
                "content process service worker fetch resume",
                "content process rejected the service worker fetch resume",
            ));
        }
        let result = decode_load_response(&response, id);
        if let Ok(NativeContentLoadResult::Loaded(content)) = &result {
            self.discard_stale_parent_event_source_streams(content.generation);
            self.current_document_url = Some(content.url.clone());
            self.current_document_generation = Some(content.generation);
            self.pending_navigation_request = None;
        }
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    /// Ask the process-backed document owner for its enforced and report-only
    /// top-level navigation result. The caller separately applies the
    /// returned report records through the normal page event channel.
    pub(crate) async fn check_navigation_policy(
        &mut self,
        document_url: &str,
        target_url: &str,
        report: bool,
    ) -> Result<NativeContentNavigationPolicy, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_MUTATION_TIMEOUT,
            self.exchange(json!({
                "kind": "navigation_policy",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "policy": "navigate-to",
                "document_url": document_url,
                "target_url": target_url,
                "report": report,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process navigation policy",
                    NativeWorkerFailureKind::Timeout,
                    "content process navigation policy exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            self.mark_failed(NativeWorkerFailureKind::Rejected);
            let _ = self.child.start_kill();
            return Err(NativeEngineError::worker_failure(
                "content process navigation policy",
                NativeWorkerFailureKind::Rejected,
                response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the navigation policy query"),
            ));
        }
        let result = decode_navigation_policy_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn mutate_click_with_event_preflight(
        &mut self,
        node_index: u32,
        modifiers: u8,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_click_preflight",
            json!({"node_index": node_index, "modifiers": modifiers}),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn mutate_type_with_event_bridge(
        &mut self,
        node_index: u32,
        text: String,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_type_events",
            json!({"node_index": node_index, "text": text}),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn mutate_form_action_with_event_bridge(
        &mut self,
        action: Value,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(id, "mutate_form_events", action, parent_loader)
            .await
    }

    pub(crate) async fn mutate_key_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_events",
            json!({"node_index": node_index, "key": key}),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn mutate_key_event_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
        kind: NativeEventKind,
        modifiers: i64,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_event",
            json!({
                "node_index": node_index,
                "key": key,
                "kind": event_kind_text(kind),
                "modifiers": modifiers,
            }),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn mutate_key_shortcut_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
        modifiers: i64,
        apply_default: bool,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_shortcut",
            json!({
                "node_index": node_index,
                "key": key,
                "kind": "shortcut",
                "modifiers": modifiers,
                "apply_default": apply_default,
            }),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn dispatch_lifecycle_events(
        &mut self,
        events: &[NativeEventKind],
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let event_names = events
            .iter()
            .map(|kind| event_kind_text(*kind))
            .collect::<Vec<_>>();
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_lifecycle_events",
            json!({"events": event_names}),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn dispatch_before_unload(
        &mut self,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(id, "mutate_before_unload", Value::Null, parent_loader)
            .await
    }

    pub(crate) async fn dispatch_hash_change(
        &mut self,
        old_url: &str,
        new_url: &str,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_hash_change",
            json!({"old_url": old_url, "new_url": new_url}),
            parent_loader,
        )
        .await
    }

    pub(crate) async fn fetch(
        &mut self,
        document_url: &str,
        href: &str,
        credentials: bool,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        let owner = NativeContentCookieOwner {
            context_id: self.context_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process fetch owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no browser context identity",
                )
            })?,
            frame_id: self.frame_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process fetch owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no frame identity",
                )
            })?,
            generation: self.current_document_generation.ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process fetch owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document generation",
                )
            })?,
            document_url: self.current_document_url.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process fetch owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document URL",
                )
            })?,
        };
        if without_fragment(&owner.document_url) != without_fragment(document_url) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "native Fetch owner does not match the committed document".into(),
            });
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange_with_parent_loader(
                json!({
                    "kind": "fetch",
                    "id": id,
                    "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                    "document_url": document_url,
                    "href": href,
                    "credentials": credentials,
                    "owner": owner,
                }),
                Some(parent_loader),
            ),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process fetch",
                    NativeWorkerFailureKind::Timeout,
                    "content process fetch exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process fetch".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the fetch")
                    .into(),
            });
        }
        let result = decode_fetch_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    #[cfg(test)]
    pub(crate) async fn evaluate_with_page_events(
        &mut self,
        source: &str,
        page_events: &NativePageEventBatch,
        parent_loader: Option<&mut NativeResourceLoader>,
    ) -> Result<NativeContentScriptResult, NativeEngineError> {
        self.evaluate_with_page_events_timeout(
            source,
            page_events,
            parent_loader,
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
        )
        .await
    }

    pub(crate) async fn evaluate_with_page_events_timeout(
        &mut self,
        source: &str,
        page_events: &NativePageEventBatch,
        parent_loader: Option<&mut NativeResourceLoader>,
        deadline: Duration,
    ) -> Result<NativeContentScriptResult, NativeEngineError> {
        let owner = NativeContentCookieOwner {
            context_id: self.context_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process script owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no browser context identity",
                )
            })?,
            frame_id: self.frame_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process script owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no frame identity",
                )
            })?,
            generation: self.current_document_generation.ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process script owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document generation",
                )
            })?,
            document_url: self.current_document_url.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process script owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document URL",
                )
            })?,
        };
        let document_cookie = parent_loader
            .as_deref()
            .map(|loader| loader.document_cookie(&owner.document_url))
            .transpose()?
            .unwrap_or_default();
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document.cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        let id = self.next_id();
        let response = match self
            .exchange_with_parent_loader_timeout(
                json!({
                "kind": "script",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "source": source,
                "page_events": page_events,
                "owner": owner,
                "document_cookie": document_cookie,
                }),
                "content process script",
                deadline,
                parent_loader,
            )
            .await
        {
            Ok(response) => response,
            Err(error) => return Err(error),
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the script")
                    .into(),
            });
        }
        let result = decode_script_response(&response, id);
        if result.is_ok() {
            // A successful script turn receives every queued event represented
            // by notifications read before its response. Its typed response
            // carries the resulting effects, so the owner need not run a
            // second empty script turn for the same notification.
            self.pending_async_effect_notifications.clear();
        }
        result
    }

    pub(crate) async fn resolve_service_worker_open_window(
        &mut self,
        worker_id: u32,
        request_id: u32,
        window: &Value,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentScriptResult, NativeEngineError> {
        if worker_id == 0 || request_id == 0 {
            return Err(NativeEngineError::invalid(
                "content-process service worker openWindow resolution",
                "request and worker ids must be positive",
            ));
        }
        let (generation, document_url) = match (
            self.current_document_generation,
            self.current_document_url.as_deref(),
        ) {
            (Some(generation), Some(document_url)) => (generation, document_url.to_owned()),
            _ => {
                // A Service Worker can call clients.openWindow while its
                // intercepted first navigation is suspended. That document
                // has a reserved generation and URL, but is not committed yet.
                let pending = self.pending_navigation_request.as_ref().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process service worker openWindow owner",
                        NativeWorkerFailureKind::Protocol,
                        "content process has no committed or pending document owner",
                    )
                })?;
                let generation = pending
                    .get("generation")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|generation| *generation != 0)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process service worker openWindow owner",
                            NativeWorkerFailureKind::Protocol,
                            "pending navigation has no valid document generation",
                        )
                    })?;
                let document_url = pending
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process service worker openWindow owner",
                            NativeWorkerFailureKind::Protocol,
                            "pending navigation has no document URL",
                        )
                    })?
                    .to_owned();
                (generation, document_url)
            }
        };
        let owner = NativeContentCookieOwner {
            context_id: self.context_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process service worker openWindow owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no browser context identity",
                )
            })?,
            frame_id: self.frame_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process service worker openWindow owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no frame identity",
                )
            })?,
            generation,
            document_url,
        };
        let id = self.next_id();
        let response = self
            .exchange_with_parent_loader_timeout(
                json!({
                    "kind": "service_worker_open_window_resolve",
                    "id": id,
                    "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                    "worker_id": worker_id,
                    "request_id": request_id,
                    "owner": owner,
                    "window": window,
                }),
                "content process service worker openWindow resolution",
                CONTENT_PROCESS_SCRIPT_TIMEOUT,
                Some(parent_loader),
            )
            .await?;
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process service worker openWindow resolution".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the service worker openWindow resolution")
                    .into(),
            });
        }
        let result = decode_script_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_scroll_offset(
        &mut self,
        scroll_offset: NativePoint,
    ) -> Result<(), NativeEngineError> {
        if self.scroll_offset == scroll_offset {
            return Ok(());
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "scroll_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "x": scroll_offset.x,
                "y": scroll_offset.y,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process scroll synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process scroll synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "scroll_synced",
            id,
            "content process scroll synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        } else {
            self.scroll_offset = scroll_offset;
        }
        result
    }

    pub(crate) async fn sync_nested_scroll_offsets(
        &mut self,
        offsets: &BTreeMap<u32, NativePoint>,
    ) -> Result<(), NativeEngineError> {
        if &self.nested_scroll_offsets == offsets {
            return Ok(());
        }
        let id = self.next_id();
        let nested = offsets
            .iter()
            .map(|(node_index, offset)| {
                json!({
                    "node_index": node_index,
                    "x": offset.x,
                    "y": offset.y,
                })
            })
            .collect::<Vec<_>>();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "scroll_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "nested": nested,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process nested scroll synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process nested scroll synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "scroll_synced",
            id,
            "content process nested scroll synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        } else {
            self.nested_scroll_offsets = offsets.clone();
        }
        result
    }

    pub(crate) async fn sync_history(
        &mut self,
        url: &str,
        state: &Value,
        length: usize,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("content process history URL", url)?;
        let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
            operation: "serialize content process history state".into(),
            reason: "history state could not be serialized".into(),
        })?;
        if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
            return Err(NativeEngineError::limit(
                "content process history state",
                MAX_NATIVE_HISTORY_STATE_BYTES,
                encoded.len(),
            ));
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "history_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "url": url,
                "state": state,
                "length": length.max(1),
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process history synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process history synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "history_synced",
            id,
            "content process history synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        } else {
            self.current_document_url = Some(url.to_owned());
        }
        result
    }

    pub(crate) async fn sync_window_proxies(
        &mut self,
        updates: &[NativeWindowProxyUpdate],
    ) -> Result<(), NativeEngineError> {
        if updates.is_empty() {
            return Ok(());
        }
        if updates.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native WindowProxy updates",
                MAX_NATIVE_EFFECTS,
                updates.len(),
            ));
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "window_proxy_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "updates": updates,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process WindowProxy synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process WindowProxy synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "window_proxy_synced",
            id,
            "content process WindowProxy synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_frame_script_bindings(
        &mut self,
        bindings: &[NativeFrameScriptBinding],
    ) -> Result<(), NativeEngineError> {
        if bindings.len() > MAX_CONTENT_FRAME_SOURCES {
            return Err(NativeEngineError::limit(
                "native frame script bindings",
                MAX_CONTENT_FRAME_SOURCES,
                bindings.len(),
            ));
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "frame_script_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "bindings": bindings,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process frame script synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process frame script synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "frame_script_synced",
            id,
            "content process frame script synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_storage_events(
        &mut self,
        events: &[NativeStorageEvent],
    ) -> Result<(), NativeEngineError> {
        if events.is_empty() {
            return Ok(());
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "storage_events",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "events": events,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process storage events",
                    NativeWorkerFailureKind::Timeout,
                    "content process storage-event synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "storage_events_synced",
            id,
            "content process storage events",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_storage_state(
        &mut self,
        state: &NativeWebStorageState,
        indexed_db_state: &NativeIndexedDbState,
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "storage_state",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "state": state,
                "indexed_db_state": indexed_db_state,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process storage state",
                    NativeWorkerFailureKind::Timeout,
                    "content process storage-state synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "storage_state_synced",
            id,
            "content process storage state",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_document_cookie_projection(
        &mut self,
        document_url: &str,
        document_cookie: &str,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("parent document.cookie projection URL", document_url)?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document.cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        if document_cookie.chars().any(char::is_control) {
            return Err(NativeEngineError::invalid(
                "parent document.cookie projection",
                "must not contain control characters",
            ));
        }
        if self
            .current_document_url
            .as_deref()
            .is_none_or(|current| without_fragment(current) != without_fragment(document_url))
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "document.cookie projection belongs to a different document".into(),
            });
        }
        let owner = NativeContentCookieOwner {
            context_id: self.context_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process document.cookie projection",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no browser context identity",
                )
            })?,
            frame_id: self.frame_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process document.cookie projection",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no frame identity",
                )
            })?,
            generation: self.current_document_generation.ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process document.cookie projection",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document generation",
                )
            })?,
            document_url: document_url.to_owned(),
        };
        let id = self.next_id();
        let response = self
            .exchange_with_timeout(
                json!({
                "kind": "document_cookie_projection",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "owner": owner,
                "document_cookie": document_cookie,
                }),
                "content process document.cookie projection",
                CONTENT_PROCESS_SCRIPT_TIMEOUT,
            )
            .await?;
        require_response_kind(
            &response,
            "document_cookie_projection_synced",
            id,
            "content process document.cookie projection",
        )
    }

    async fn mutate_with_request_kind(
        &mut self,
        id: u64,
        request_kind: &str,
        action: Value,
        parent_loader: &mut NativeResourceLoader,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let mutation_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE")
            .map(|_| ContentLoadStage::begin(id, "content_process_mutation"));
        let owner = NativeContentCookieOwner {
            context_id: self.context_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process mutation owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no browser context identity",
                )
            })?,
            frame_id: self.frame_id.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process mutation owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no frame identity",
                )
            })?,
            generation: self.current_document_generation.ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process mutation owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document generation",
                )
            })?,
            document_url: self.current_document_url.clone().ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process mutation owner",
                    NativeWorkerFailureKind::Protocol,
                    "content process has no committed document URL",
                )
            })?,
        };
        let response = self
            .exchange_with_parent_loader_timeout(
                json!({
                "kind": request_kind,
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "owner": owner,
                "action": action,
                }),
                "content process mutation",
                CONTENT_PROCESS_MUTATION_TIMEOUT,
                Some(parent_loader),
            )
            .await?;
        if let Some(stage) = mutation_stage {
            stage.finish(None);
        }
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            self.mark_failed(NativeWorkerFailureKind::Rejected);
            let _ = self.child.start_kill();
            return Err(NativeEngineError::worker_failure(
                "content process mutation",
                NativeWorkerFailureKind::Rejected,
                response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the mutation"),
            ));
        }
        let result = decode_mutated_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    /// Refresh the cached health bit without writing to the worker channel.
    ///
    /// A worker may exit between two successful exchanges. Keeping the
    /// previous boolean forever would make recovery miss that dead owner and
    /// would turn the next operation into an avoidable transport failure.
    pub(crate) fn refresh_health(&mut self) -> bool {
        if !self.healthy {
            return false;
        }
        if self.ipc_aborted.load(Ordering::Acquire) {
            self.mark_failed(NativeWorkerFailureKind::Exited);
            return false;
        }
        match self.child.try_wait() {
            Ok(Some(_)) => {
                self.mark_failed(NativeWorkerFailureKind::Exited);
                false
            }
            Ok(None) => true,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Transport);
                false
            }
        }
    }

    pub(crate) fn failure_kind(&self) -> Option<NativeWorkerFailureKind> {
        self.failure_kind
    }

    pub(crate) async fn terminate_and_reap(
        &mut self,
        operation: &str,
    ) -> Result<(), NativeEngineError> {
        self.mark_failed(NativeWorkerFailureKind::Exited);
        let _ = self.child.start_kill();
        match timeout(Duration::from_secs(2), self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(NativeEngineError::worker_failure(
                operation,
                NativeWorkerFailureKind::Exited,
                "content process could not be reaped after cancellation",
            )),
            Err(_) => Err(NativeEngineError::worker_failure(
                operation,
                NativeWorkerFailureKind::Exited,
                "content process did not exit within the cancellation deadline",
            )),
        }
    }

    pub(crate) async fn close(&mut self) -> Result<(), NativeEngineError> {
        if let Ok(Some(_)) = self.child.try_wait() {
            self.mark_failed(NativeWorkerFailureKind::Exited);
            return Ok(());
        }
        let id = self.next_id();
        let response = match timeout(
            Duration::from_secs(2),
            self.exchange(json!({
                "kind": "close",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            })),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                self.terminate_and_reap("content process close").await?;
                return Err(error);
            }
            Err(_) => {
                self.terminate_and_reap("content process close").await?;
                self.parent_event_source_streams.clear();
                self.parent_websocket_streams.clear();
                return Ok(());
            }
        };
        require_response_kind(&response, "closed", id, "content process close")?;
        self.parent_event_source_streams.clear();
        self.parent_websocket_streams.clear();
        match timeout(Duration::from_secs(1), self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(NativeEngineError::Worker {
                operation: "wait for content process".into(),
                reason: "content process did not report a valid exit".into(),
            }),
            Err(_) => {
                let _ = self.child.start_kill();
                Ok(())
            }
        }
    }

    async fn exchange(&mut self, request: Value) -> Result<Value, NativeEngineError> {
        self.exchange_with_parent_loader(request, None).await
    }

    async fn exchange_with_parent_loader(
        &mut self,
        request: Value,
        parent_loader: Option<&mut NativeResourceLoader>,
    ) -> Result<Value, NativeEngineError> {
        let mut abort_child_on_drop =
            KillContentChildOnDrop::new(self.child.id(), Arc::clone(&self.ipc_aborted));
        let result = self.exchange_inner(request, parent_loader).await;
        abort_child_on_drop.disarm();
        match result {
            Ok(response) => {
                if let Err(error) = self.collect_cookie_changes(&response) {
                    self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
                    let _ = self.child.start_kill();
                    Err(error)
                } else if let Err(error) = self.collect_cookie_writes(&response) {
                    self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
                    let _ = self.child.start_kill();
                    Err(error)
                } else {
                    Ok(response)
                }
            }
            Err(error) => {
                let kind = match self.child.try_wait() {
                    Ok(Some(_)) => NativeWorkerFailureKind::Exited,
                    Ok(None) | Err(_) => NativeWorkerFailureKind::Transport,
                };
                self.mark_failed(kind);
                Err(NativeEngineError::worker_failure(
                    "content process IPC",
                    kind,
                    error.to_string(),
                ))
            }
        }
    }

    fn accept_async_effects_ready(&mut self, value: &Value) -> Result<bool, NativeEngineError> {
        let Some(event) = decode_async_effects_ready(
            value,
            self.next_async_event_sequence,
            self.context_id.as_deref(),
            self.frame_id.as_deref(),
        )?
        else {
            return Ok(false);
        };
        if self.pending_async_effect_notifications.len() >= MAX_CONTENT_ASYNC_EFFECT_NOTIFICATIONS {
            return Err(NativeEngineError::worker_failure(
                "content process asynchronous event",
                NativeWorkerFailureKind::Protocol,
                "content process exceeded the pending asynchronous notification limit",
            ));
        }
        self.next_async_event_sequence = event.sequence.checked_add(1).ok_or_else(|| {
            NativeEngineError::worker_failure(
                "content process asynchronous event",
                NativeWorkerFailureKind::Protocol,
                "content process asynchronous event sequence is exhausted",
            )
        })?;
        self.pending_async_effect_notifications
            .push_back(NativeContentAsyncEffectNotification {
                sequence: event.sequence,
                context_id: event.context_id,
                frame_id: event.frame_id,
            });
        Ok(true)
    }

    pub(crate) fn take_async_effect_notifications(
        &mut self,
    ) -> Result<Vec<NativeContentAsyncEffectNotification>, NativeEngineError> {
        loop {
            match self.stdout_frames.try_recv() {
                Ok(Ok(payload)) => {
                    let value: Value = serde_json::from_slice(&payload).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "content process asynchronous event",
                            NativeWorkerFailureKind::Protocol,
                            "content process emitted invalid JSON while idle",
                        )
                    })?;
                    if !self.accept_async_effects_ready(&value)? {
                        return Err(NativeEngineError::worker_failure(
                            "content process asynchronous event",
                            NativeWorkerFailureKind::Protocol,
                            "content process emitted an unsolicited response while idle",
                        ));
                    }
                }
                Ok(Err(error)) => return Err(error),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(NativeEngineError::worker_failure(
                        "content process asynchronous event",
                        NativeWorkerFailureKind::Exited,
                        "content process output stream closed",
                    ));
                }
            }
        }
        Ok(self.pending_async_effect_notifications.drain(..).collect())
    }

    async fn read_response_value(&mut self) -> Result<Value, NativeEngineError> {
        loop {
            let payload = self.stdout_frames.recv().await.ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process IPC",
                    NativeWorkerFailureKind::Transport,
                    "content process output reader stopped",
                )
            })??;
            let value: Value = serde_json::from_slice(&payload).map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode content IPC",
                    NativeWorkerFailureKind::Protocol,
                    "content process returned invalid JSON",
                )
            })?;
            if self.accept_async_effects_ready(&value)? {
                continue;
            }
            return Ok(value);
        }
    }

    fn collect_cookie_changes(&mut self, response: &Value) -> Result<(), NativeEngineError> {
        let Some(value) = response.get("cookie_changes") else {
            return Ok(());
        };
        let changes: Vec<NativeCookieChange> =
            serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
                operation: "decode content process cookie changes".into(),
                reason: "content process returned an invalid cookie change journal".into(),
            })?;
        if !changes.is_empty() {
            return Err(NativeEngineError::worker_failure(
                "content process cookie authority",
                NativeWorkerFailureKind::InvalidTransfer,
                "a content process cannot publish cookie changes; HTTP(S) requests must use the parent broker",
            ));
        }
        let mut pending =
            self.pending_cookie_changes
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue content process cookie changes".into(),
                    reason: "content process cookie change queue is poisoned".into(),
                })?;
        merge_cookie_change_batch(&mut pending, changes, "content process cookie changes")
    }

    fn collect_cookie_writes(&mut self, response: &Value) -> Result<(), NativeEngineError> {
        let Some(value) = response.get("cookie_writes") else {
            return Ok(());
        };
        let writes: Vec<NativeContentCookieWrite> =
            serde_json::from_value(value.clone()).map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode content process cookie writes",
                    NativeWorkerFailureKind::InvalidTransfer,
                    "content process returned an invalid cookie-write journal",
                )
            })?;
        if writes.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content process cookie writes",
                MAX_NATIVE_EFFECTS,
                writes.len(),
            ));
        }
        let pending_len = self
            .pending_cookie_writes
            .len()
            .saturating_add(writes.len());
        if pending_len > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "pending content process cookie writes",
                MAX_NATIVE_EFFECTS,
                pending_len,
            ));
        }
        let mut total_bytes = self
            .pending_cookie_writes
            .iter()
            .map(|write| write.value.len())
            .sum::<usize>();
        for write in &writes {
            if write.value.len() > crate::limits::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "content process cookie write",
                    crate::limits::MAX_TEXT_BYTES,
                    write.value.len(),
                ));
            }
            total_bytes = total_bytes.saturating_add(write.value.len());
        }
        if total_bytes > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "pending content process cookie writes",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                total_bytes,
            ));
        }
        self.pending_cookie_writes.extend(writes);
        Ok(())
    }

    pub(crate) fn take_pending_cookie_writes(
        &mut self,
    ) -> Result<Vec<NativeContentCookieWrite>, NativeEngineError> {
        let writes = std::mem::take(&mut self.pending_cookie_writes);
        for write in &writes {
            let owner = &write.owner;
            if self.context_id.as_deref() != Some(owner.context_id.as_str())
                || self.frame_id.as_deref() != Some(owner.frame_id.as_str())
                || self.current_document_generation != Some(owner.generation)
                || self.current_document_url.as_deref().is_none_or(|current| {
                    without_fragment(current) != without_fragment(&owner.document_url)
                })
            {
                return Err(NativeEngineError::worker_failure(
                    "apply content process cookie writes",
                    NativeWorkerFailureKind::InvalidTransfer,
                    "cookie write belongs to a stale or different document owner",
                ));
            }
        }
        Ok(writes)
    }

    async fn exchange_with_timeout(
        &mut self,
        request: Value,
        operation: &str,
        deadline: Duration,
    ) -> Result<Value, NativeEngineError> {
        self.exchange_with_parent_loader_timeout(request, operation, deadline, None)
            .await
    }

    async fn exchange_with_parent_loader_timeout(
        &mut self,
        request: Value,
        operation: &str,
        deadline: Duration,
        parent_loader: Option<&mut NativeResourceLoader>,
    ) -> Result<Value, NativeEngineError> {
        let dialog_control = self.dialog_control.clone();
        let context_id = self.context_id.clone();
        let frame_id = self.frame_id.clone();
        let mut exchange = Box::pin(self.exchange_with_parent_loader(request, parent_loader));
        let mut timer = Box::pin(sleep(deadline));
        loop {
            tokio::select! {
                biased;
                response = &mut exchange => return response,
                _ = &mut timer => {
                    let pending = match dialog_control.pending() {
                        Ok(pending) => pending,
                        Err(error) => {
                            drop(exchange);
                            self.mark_failed(NativeWorkerFailureKind::Transport);
                            let _ = self.child.start_kill();
                            return Err(error);
                        }
                    };
                    let suspending_dialog = pending.filter(|pending| {
                        context_id.as_deref() == Some(pending.context_id.as_str())
                            && frame_id.as_deref() == Some(pending.frame_id.as_str())
                    });
                    if let Some(pending) = suspending_dialog {
                        let wait_for_resolution =
                            dialog_control.wait_for_resolution_of(&pending.id);
                        tokio::pin!(wait_for_resolution);
                        tokio::select! {
                            biased;
                            response = &mut exchange => return response,
                            result = &mut wait_for_resolution => {
                                if let Err(error) = result {
                                    drop(exchange);
                                    self.mark_failed(NativeWorkerFailureKind::Transport);
                                    let _ = self.child.start_kill();
                                    return Err(error);
                                }
                                timer.as_mut().reset(tokio::time::Instant::now() + deadline);
                            }
                        }
                        continue;
                    }
                    drop(exchange);
                    self.mark_failed(NativeWorkerFailureKind::Timeout);
                    let _ = self.child.start_kill();
                    return Err(NativeEngineError::worker_failure(
                        operation,
                        NativeWorkerFailureKind::Timeout,
                        "content process operation exceeded its deadline",
                    ));
                }
            }
        }
    }

    fn mark_failed(&mut self, kind: NativeWorkerFailureKind) {
        self.healthy = false;
        self.failure_kind = Some(kind);
        self.parent_event_source_streams.clear();
        self.parent_websocket_streams.clear();
    }

    fn discard_stale_parent_event_source_streams(&mut self, generation: u32) {
        let (Some(context_id), Some(frame_id)) =
            (self.context_id.as_deref(), self.frame_id.as_deref())
        else {
            return;
        };
        let context_id = context_id.to_owned();
        let frame_id = frame_id.to_owned();
        self.parent_event_source_streams.retain(|_, stream| {
            stream.owner.context_id != context_id
                || stream.owner.frame_id != frame_id
                || stream.owner.generation == generation
        });
        self.parent_websocket_streams.retain(|_, stream| {
            stream.owner.context_id != context_id
                || stream.owner.frame_id != frame_id
                || stream.owner.generation == generation
        });
    }

    async fn handle_parent_navigation_request(
        &mut self,
        request_id: u64,
        load_request: &Value,
        broker_request: &Value,
        parent_loader: Option<&mut NativeResourceLoader>,
    ) -> Result<Option<String>, NativeEngineError> {
        let loader = parent_loader.ok_or_else(|| {
            NativeEngineError::worker_failure(
                "content process parent navigation broker",
                NativeWorkerFailureKind::Protocol,
                "content process requested parent navigation without a broker",
            )
        })?;
        let broker_response = Box::pin(parent_navigation_broker_response(
            request_id,
            load_request,
            broker_request,
            self.context_id.as_deref(),
            self.frame_id.as_deref(),
            loader,
        ))
        .await?;
        let document_url = broker_response
            .get("url")
            .and_then(Value::as_str)
            .filter(|_| {
                broker_response.get("kind").and_then(Value::as_str)
                    == Some("parent_navigation_response")
            })
            .map(str::to_owned);
        let payload = serde_json::to_vec(&broker_response).map_err(|_| {
            NativeEngineError::worker_failure(
                "encode parent navigation response",
                NativeWorkerFailureKind::Protocol,
                "parent navigation response could not be encoded",
            )
        })?;
        write_frame(&mut self.stdin, &payload).await?;
        Ok(document_url)
    }

    async fn acknowledge_parent_fetch_cancellation(
        &mut self,
        request_id: u64,
        fetch_id: u32,
    ) -> Result<(), NativeEngineError> {
        let payload = serde_json::to_vec(&json!({
            "kind": "parent_fetch_cancelled",
            "id": request_id,
            "fetch_id": fetch_id,
        }))
        .map_err(|_| {
            NativeEngineError::worker_failure(
                "encode parent Fetch cancellation acknowledgement",
                NativeWorkerFailureKind::Protocol,
                "cancellation acknowledgement could not be encoded",
            )
        })?;
        write_frame(&mut self.stdin, &payload).await
    }

    fn apply_parent_page_meta_csp(
        &mut self,
        loader: &mut NativeResourceLoader,
        owner: &NativeContentCookieOwner,
        policies: &[String],
    ) -> Result<(), NativeEngineError> {
        let owner_changed = self.parent_brokered_policy_owner.as_ref() != Some(owner);
        let new_policies = policies
            .iter()
            .filter(|policy| owner_changed || !self.parent_brokered_meta_policies.contains(*policy))
            .cloned()
            .collect::<Vec<_>>();
        loader.append_meta_content_security_policies(&owner.document_url, &new_policies)?;
        if owner_changed {
            self.parent_brokered_policy_owner = Some(owner.clone());
            self.parent_brokered_meta_policies.clear();
        }
        self.parent_brokered_meta_policies
            .extend(policies.iter().cloned());
        Ok(())
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }
}
