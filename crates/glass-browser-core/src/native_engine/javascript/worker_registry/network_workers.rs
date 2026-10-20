use super::*;

impl NativeWorkerRegistry {
    pub async fn dispatch_websocket_event(
        &mut self,
        worker_id: u32,
        socket_id: u32,
        event: &serde_json::Value,
        csp_violations: &[NativeCspViolation],
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        if !self.workers.contains_key(&worker_id) {
            return Ok(());
        }
        let evaluation = self.evaluate_worker_with_loader(worker_id, loader, |worker| {
            worker.evaluate_websocket_event(worker_id, socket_id, event, csp_violations)
        });
        match evaluation {
            Ok(evaluation) => {
                self.collect_worker_evaluation(worker_id, evaluation, loader)
                    .await
            }
            Err(error) => {
                let worker_url = self
                    .workers
                    .get(&worker_id)
                    .map(|worker| worker.url.clone())
                    .unwrap_or_default();
                self.workers.remove(&worker_id);
                self.remove_worker_routes(worker_id);
                self.queue_error(worker_id, &worker_url, &error.to_string())
            }
        }
    }

    pub async fn dispatch_event_source_event(
        &mut self,
        worker_id: u32,
        source_id: u32,
        event: &serde_json::Value,
        csp_violations: &[NativeCspViolation],
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        if !self.workers.contains_key(&worker_id) {
            return Ok(());
        }
        let evaluation = self.evaluate_worker_with_loader(worker_id, loader, |worker| {
            worker.evaluate_event_source_event(worker_id, source_id, event, csp_violations)
        });
        match evaluation {
            Ok(evaluation) => {
                self.collect_worker_evaluation(worker_id, evaluation, loader)
                    .await
            }
            Err(error) => {
                let worker_url = self
                    .workers
                    .get(&worker_id)
                    .map(|worker| worker.url.clone())
                    .unwrap_or_default();
                self.workers.remove(&worker_id);
                self.remove_worker_routes(worker_id);
                self.queue_error(worker_id, &worker_url, &error.to_string())
            }
        }
    }

    /// Deliver one host-owned worker Fetch response-stream event into its
    /// persistent worker realm. Worker stream reads and cancellation commands
    /// are collected by the same serialized owner as ordinary worker fetches.
    pub(crate) async fn pump_fetch_stream_event(
        &mut self,
        loader: &mut NativeResourceLoader,
    ) -> Result<bool, NativeEngineError> {
        self.pump_fetch_stream_event_with_parent_fetch_broker(loader, None)
            .await
    }

    async fn pump_fetch_stream_event_with_parent_fetch_broker<'broker>(
        &mut self,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<bool, NativeEngineError> {
        let stream_keys = self
            .worker_fetch_stream_connections
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for stream_key @ (worker_id, stream_id) in stream_keys {
            let event = {
                let Some(connection) = self.worker_fetch_stream_connections.get_mut(&stream_key)
                else {
                    continue;
                };
                match connection.events.try_recv() {
                    Ok(event) => {
                        connection.read_pending = false;
                        Some(event)
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => None,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        self.worker_fetch_stream_connections.remove(&stream_key);
                        None
                    }
                }
            };
            let Some(event) = event else {
                continue;
            };
            let remove_after_dispatch = matches!(
                &event,
                NativeFetchStreamEvent::End | NativeFetchStreamEvent::Error { .. }
            );
            let payload = worker_fetch_stream_event_payload(&event);
            let evaluation = if self.workers.contains_key(&worker_id) {
                Some(
                    self.evaluate_worker_with_loader(worker_id, loader, |worker| {
                        worker.evaluate_fetch_stream_event(worker_id, stream_id, &payload)
                    }),
                )
            } else {
                None
            };
            match evaluation {
                Some(Ok(evaluation)) => {
                    self.collect_worker_evaluation_with_parent_fetch_broker(
                        worker_id,
                        evaluation,
                        loader,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                }
                Some(Err(error)) => {
                    let worker_url = self
                        .workers
                        .get(&worker_id)
                        .map(|worker| worker.url.clone())
                        .unwrap_or_default();
                    self.workers.remove(&worker_id);
                    self.remove_worker_routes(worker_id);
                    self.queue_error(worker_id, &worker_url, &error.to_string())?;
                }
                None => {
                    if let Some(connection) =
                        self.worker_fetch_stream_connections.remove(&stream_key)
                    {
                        let _ = connection
                            .commands
                            .try_send(NativeFetchStreamCommand::Cancel);
                    }
                }
            }
            if remove_after_dispatch
                && let Some(connection) = self.worker_fetch_stream_connections.remove(&stream_key)
            {
                let _ = connection
                    .commands
                    .try_send(NativeFetchStreamCommand::Cancel);
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) async fn pump_fetch_stream_events_with_parent_fetch_broker(
        &mut self,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<usize, NativeEngineError> {
        self.pump_fetch_stream_events_with_broker(loader, Some(parent_fetch_broker))
            .await
    }

    async fn pump_fetch_stream_events_with_broker<'broker>(
        &mut self,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<usize, NativeEngineError> {
        let mut pumped = 0usize;
        for _ in 0..MAX_NATIVE_WORKER_MESSAGES {
            if parent_fetch_broker.is_some() {
                tokio::task::yield_now().await;
            }
            let read_pending = self
                .worker_fetch_stream_connections
                .values()
                .any(|connection| connection.read_pending);
            if !self
                .pump_fetch_stream_event_with_parent_fetch_broker(
                    loader,
                    parent_fetch_broker.as_deref_mut(),
                )
                .await?
            {
                if parent_fetch_broker.is_some() && read_pending {
                    continue;
                }
                break;
            }
            pumped = pumped.saturating_add(1);
            tokio::task::yield_now().await;
        }
        Ok(pumped)
    }

    pub(super) fn process_worker_fetch_stream_command(
        &mut self,
        worker_id: u32,
        command: NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        match command {
            NativeScriptCommand::FetchStreamRead {
                stream_id,
                worker_id: Some(command_worker_id),
            } if command_worker_id == worker_id => {
                let stream_key = (worker_id, stream_id);
                let connection = self
                    .worker_fetch_stream_connections
                    .get_mut(&stream_key)
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "worker fetch response stream read".into(),
                        reason: "worker fetch response stream identifier is not active".into(),
                    })?;
                if connection.read_pending {
                    return Err(NativeEngineError::Network {
                        operation: "worker fetch response stream read".into(),
                        reason: "worker fetch response stream already has a pending read".into(),
                    });
                }
                connection.read_pending = true;
                connection
                    .commands
                    .try_send(NativeFetchStreamCommand::Read)
                    .map_err(|_| NativeEngineError::Network {
                        operation: "worker fetch response stream read".into(),
                        reason: "worker fetch response stream task is unavailable".into(),
                    })?;
                Ok(())
            }
            NativeScriptCommand::FetchStreamCancel {
                stream_id,
                worker_id: Some(command_worker_id),
            } if command_worker_id == worker_id => {
                if let Some(connection) = self
                    .worker_fetch_stream_connections
                    .remove(&(worker_id, stream_id))
                {
                    let _ = connection
                        .commands
                        .try_send(NativeFetchStreamCommand::Cancel);
                }
                Ok(())
            }
            _ => Err(NativeEngineError::invalid(
                "native Worker fetch response stream command",
                "worker id does not match the owning worker",
            )),
        }
    }

    pub(super) fn process_worker_fetch_upload_command(
        &mut self,
        worker_id: u32,
        command: NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        match command {
            NativeScriptCommand::FetchUploadChunk {
                stream_id,
                data_base64,
                worker_id: Some(command_worker_id),
            } if command_worker_id == worker_id => {
                let upload_key = (worker_id, stream_id);
                let connection = self
                    .worker_fetch_upload_connections
                    .get_mut(&upload_key)
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "Worker fetch request upload chunk".into(),
                        reason: "Worker fetch request upload identifier is not active".into(),
                    })?;
                if !connection.demand_pending {
                    return Err(NativeEngineError::Network {
                        operation: "Worker fetch request upload chunk".into(),
                        reason: "Worker fetch request upload chunk arrived without demand".into(),
                    });
                }
                let data = base64::engine::general_purpose::STANDARD
                    .decode(data_base64)
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "Worker fetch request upload chunk",
                            "must be valid base64",
                        )
                    })?;
                let next_total = connection.total_bytes.saturating_add(data.len());
                let next_chunks = connection.chunk_count.saturating_add(1);
                if next_total > MAX_NATIVE_FORM_BODY_BYTES
                    || next_chunks > MAX_NATIVE_FETCH_UPLOAD_CHUNKS
                {
                    let connection = self
                        .worker_fetch_upload_connections
                        .remove(&upload_key)
                        .expect("Worker fetch upload connection was checked above");
                    let message = if next_total > MAX_NATIVE_FORM_BODY_BYTES {
                        "native Worker fetch request upload body exceeds its limit"
                    } else {
                        "native Worker fetch request upload chunk limit exceeded"
                    };
                    let _ = connection
                        .commands
                        .try_send(NativeFetchUploadCommand::Error {
                            message: message.into(),
                        });
                    return Ok(());
                }
                let connection = self
                    .worker_fetch_upload_connections
                    .get_mut(&upload_key)
                    .expect("Worker fetch upload connection was checked above");
                connection.total_bytes = next_total;
                connection.chunk_count = next_chunks;
                connection.demand_pending = false;
                connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Chunk { data })
                    .map_err(|_| NativeEngineError::Network {
                        operation: "Worker fetch request upload chunk".into(),
                        reason: "Worker fetch request upload task is unavailable".into(),
                    })?;
                Ok(())
            }
            NativeScriptCommand::FetchUploadEnd {
                stream_id,
                worker_id: Some(command_worker_id),
            } if command_worker_id == worker_id => {
                let upload_key = (worker_id, stream_id);
                let mut connection = self
                    .worker_fetch_upload_connections
                    .remove(&upload_key)
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "Worker fetch request upload end".into(),
                        reason: "Worker fetch request upload identifier is not active".into(),
                    })?;
                if !connection.demand_pending {
                    return Err(NativeEngineError::Network {
                        operation: "Worker fetch request upload end".into(),
                        reason: "Worker fetch request upload ended without demand".into(),
                    });
                }
                connection.demand_pending = false;
                connection
                    .commands
                    .try_send(NativeFetchUploadCommand::End)
                    .map_err(|_| NativeEngineError::Network {
                        operation: "Worker fetch request upload end".into(),
                        reason: "Worker fetch request upload task is unavailable".into(),
                    })?;
                Ok(())
            }
            NativeScriptCommand::FetchUploadError {
                stream_id,
                message,
                worker_id: Some(command_worker_id),
            } if command_worker_id == worker_id => {
                let connection = self
                    .worker_fetch_upload_connections
                    .remove(&(worker_id, stream_id))
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "Worker fetch request upload error".into(),
                        reason: "Worker fetch request upload identifier is not active".into(),
                    })?;
                let message = message
                    .chars()
                    .take(crate::limits::MAX_TEXT_BYTES)
                    .collect();
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Error { message });
                Ok(())
            }
            NativeScriptCommand::FetchUploadCancel {
                stream_id,
                worker_id: Some(command_worker_id),
            } if command_worker_id == worker_id => {
                if let Some(connection) = self
                    .worker_fetch_upload_connections
                    .remove(&(worker_id, stream_id))
                {
                    let _ = connection
                        .commands
                        .try_send(NativeFetchUploadCommand::Cancel);
                }
                Ok(())
            }
            _ => Err(NativeEngineError::invalid(
                "native Worker fetch request upload command",
                "worker id does not match the owning worker",
            )),
        }
    }

    pub(super) fn queue_websocket_command(
        &mut self,
        worker_id: u32,
        command: NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        let worker_url = self
            .workers
            .get(&worker_id)
            .map(|worker| worker.url.clone())
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native Worker WebSocket command",
                    "worker no longer exists",
                )
            })?;
        if self.pending_websocket_commands.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Worker WebSocket commands",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_websocket_commands.len().saturating_add(1),
            ));
        }
        self.pending_websocket_commands
            .push_back(NativeWorkerWebSocketCommand {
                worker_id,
                worker_url,
                command,
            });
        Ok(())
    }

    pub(super) fn queue_event_source_command(
        &mut self,
        worker_id: u32,
        command: NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        let worker_url = self
            .workers
            .get(&worker_id)
            .map(|worker| worker.url.clone())
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native Worker EventSource command",
                    "worker no longer exists",
                )
            })?;
        if self.pending_event_source_commands.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Worker EventSource commands",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_event_source_commands.len().saturating_add(1),
            ));
        }
        self.pending_event_source_commands
            .push_back(NativeWorkerEventSourceCommand {
                worker_id,
                worker_url,
                command,
            });
        Ok(())
    }

    pub(super) fn cancel_worker_fetch_upload_connection(&mut self, upload_key: (u32, u32)) {
        if let Some(connection) = self.worker_fetch_upload_connections.remove(&upload_key) {
            let _ = connection
                .commands
                .try_send(NativeFetchUploadCommand::Cancel);
        }
    }

    /// Drive a worker-owned request-body task until it completes. The worker
    /// realm remains the sole producer: every body read is admitted by one
    /// demand event, then routed back through the normal serialized worker
    /// evaluation. HTTP and fixture owners share this loop so their
    /// cancellation and teardown behavior cannot drift.
    async fn drive_worker_fetch_upload_task<T>(
        &mut self,
        worker_id: u32,
        request_id: u32,
        task: tokio::task::JoinHandle<T>,
        loader: &mut NativeResourceLoader,
        task_operation: &str,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<T, NativeEngineError>
    where
        T: Send + 'static,
    {
        loop {
            if task.is_finished() {
                return task.await.map_err(|_| {
                    NativeEngineError::worker_failure(
                        task_operation,
                        NativeWorkerFailureKind::Transport,
                        "native Worker fetch request upload task terminated unexpectedly",
                    )
                });
            }
            if !self.workers.contains_key(&worker_id) {
                task.abort();
                self.cancel_worker_fetch_upload_connection((worker_id, request_id));
                return Err(NativeEngineError::Worker {
                    operation: "Worker fetch request upload".into(),
                    reason: "worker terminated while its request body was in flight".into(),
                });
            }

            let mut disconnected = false;
            let demand = if let Some(connection) = self
                .worker_fetch_upload_connections
                .get_mut(&(worker_id, request_id))
            {
                match connection.events.try_recv() {
                    Ok(NativeFetchUploadEvent::Demand) => {
                        connection.demand_pending = true;
                        true
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => false,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        false
                    }
                }
            } else {
                false
            };
            if disconnected {
                task.abort();
                self.cancel_worker_fetch_upload_connection((worker_id, request_id));
                return Err(NativeEngineError::Network {
                    operation: "Worker fetch request upload".into(),
                    reason: "Worker fetch request upload task is unavailable".into(),
                });
            }
            if demand {
                let evaluation = if self.workers.contains_key(&worker_id) {
                    self.evaluate_worker_with_loader(worker_id, loader, |worker| {
                        worker.evaluate_fetch_upload_event(
                            worker_id,
                            request_id,
                            &serde_json::json!({"type": "demand"}),
                        )
                    })
                } else {
                    task.abort();
                    self.cancel_worker_fetch_upload_connection((worker_id, request_id));
                    return Err(NativeEngineError::Worker {
                        operation: "Worker fetch request upload".into(),
                        reason: "worker terminated while its request body was in flight".into(),
                    });
                };
                match evaluation {
                    Ok(evaluation) => {
                        if let Err(error) =
                            Box::pin(self.collect_worker_evaluation_with_parent_fetch_broker(
                                worker_id,
                                evaluation,
                                loader,
                                parent_fetch_broker.as_deref_mut(),
                            ))
                            .await
                        {
                            task.abort();
                            self.cancel_worker_fetch_upload_connection((worker_id, request_id));
                            return Err(error);
                        }
                    }
                    Err(error) => {
                        task.abort();
                        self.cancel_worker_fetch_upload_connection((worker_id, request_id));
                        return Err(error);
                    }
                }
            } else {
                tokio::task::yield_now().await;
            }
        }
    }

    async fn open_worker_fetch_upload(
        &mut self,
        request: NativeWorkerFetchUploadRequest,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<Result<NativeFetchResponseStream, NativeEngineError>, NativeEngineError> {
        let NativeWorkerFetchUploadRequest {
            worker_id,
            request_id,
            worker_url,
            href,
            method,
            headers,
            content_type,
            credentials,
            credentials_mode,
            referrer_url,
            referrer_policy,
            cors_mode,
            redirect_mode,
            cache_mode,
            timeout,
        } = request;
        let upload_key = (worker_id, request_id);
        if self
            .worker_fetch_upload_connections
            .contains_key(&upload_key)
        {
            return Err(NativeEngineError::Network {
                operation: "Worker fetch request upload".into(),
                reason: "Worker fetch request upload identifier is already active".into(),
            });
        }
        if self.worker_fetch_upload_connections.len() >= MAX_NATIVE_WORKER_FETCH_UPLOADS {
            return Err(NativeEngineError::limit(
                "native Worker fetch request uploads",
                MAX_NATIVE_WORKER_FETCH_UPLOADS,
                self.worker_fetch_upload_connections.len().saturating_add(1),
            ));
        }
        let (upload_connection, upload_source) = spawn_native_fetch_upload_source();
        self.worker_fetch_upload_connections
            .insert(upload_key, upload_connection);
        if is_parent_owned_network_target(&worker_url, &href)
            && parent_fetch_broker.is_none()
            && !self.parent_loader_owns_network
        {
            self.cancel_worker_fetch_upload_connection(upload_key);
            return Ok(Err(missing_parent_network_authority("Worker Fetch upload")));
        }
        if parent_fetch_broker.is_some() {
            let task = tokio::spawn(async move {
                upload_source
                    .collect(MAX_NATIVE_FORM_BODY_BYTES)
                    .await
                    .map_err(|reason| NativeEngineError::Network {
                        operation: "parent-brokered Worker fetch request upload".into(),
                        reason,
                    })
            });
            let body = self
                .drive_worker_fetch_upload_task(
                    worker_id,
                    request_id,
                    task,
                    loader,
                    "parent-brokered Worker fetch request upload task",
                    parent_fetch_broker.as_deref_mut(),
                )
                .await;
            self.cancel_worker_fetch_upload_connection(upload_key);
            let body = body??;
            let request = NativeFetchRequest {
                document_url: &worker_url,
                href: &href,
                method,
                body: Some(NativeRequestBody::Bytes(body)),
                content_type,
                request_headers: headers,
                credentials,
                credentials_mode: Some(credentials_mode),
                referrer_url,
                referrer_policy,
                cors_mode,
                redirect_mode,
                cache_mode,
                timeout,
                max_response_bytes: None,
            };
            let response = parent_fetch_broker
                .as_deref_mut()
                .expect("parent Worker Fetch broker was checked above")
                .fetch(request_id, &request)
                .await?
                .0;
            let opened = response.map(|mut response| {
                let cached_body = Some(std::mem::take(&mut response.body));
                NativeFetchResponseStream {
                    response,
                    body: None,
                    cached_body,
                    max_response_bytes: MAX_NATIVE_FETCH_STREAM_BODY_BYTES,
                }
            });
            return Ok(opened);
        }
        if is_network_url(&worker_url) {
            let request_body = upload_source.into_body();
            let task_loader = loader.clone();
            let parent_loader_owns_network = self.parent_loader_owns_network;
            let task = tokio::spawn(async move {
                let mut task_loader = task_loader;
                let request = NativeFetchRequest {
                    document_url: &worker_url,
                    href: &href,
                    method,
                    body: None,
                    content_type,
                    request_headers: headers,
                    credentials,
                    credentials_mode: Some(credentials_mode),
                    referrer_url,
                    referrer_policy,
                    cors_mode,
                    redirect_mode,
                    cache_mode,
                    timeout,
                    max_response_bytes: Some(MAX_NATIVE_FETCH_STREAM_BODY_BYTES),
                };
                let result = if parent_loader_owns_network {
                    task_loader
                        .fetch_request_with_body_async(request, request_body)
                        .await
                        .map(|mut response| {
                            let cached_body = Some(std::mem::take(&mut response.body));
                            NativeFetchResponseStream {
                                response,
                                body: None,
                                cached_body,
                                max_response_bytes: MAX_NATIVE_FETCH_STREAM_BODY_BYTES,
                            }
                        })
                } else {
                    task_loader
                        .open_fetch_response_stream_with_body_async(request, Some(request_body))
                        .await
                };
                (result, task_loader)
            });
            let (opened, task_loader) = self
                .drive_worker_fetch_upload_task(
                    worker_id,
                    request_id,
                    task,
                    loader,
                    "Worker fetch request upload task",
                    None,
                )
                .await?;
            self.cancel_worker_fetch_upload_connection(upload_key);
            loader.merge_fetch_task_state(task_loader)?;
            return Ok(opened);
        }

        let task = tokio::spawn(async move {
            upload_source
                .collect(MAX_NATIVE_FORM_BODY_BYTES)
                .await
                .map_err(|reason| NativeEngineError::Network {
                    operation: "fixture Worker fetch request upload".into(),
                    reason,
                })
        });
        let body = self
            .drive_worker_fetch_upload_task(
                worker_id,
                request_id,
                task,
                loader,
                "fixture Worker fetch request upload task",
                parent_fetch_broker,
            )
            .await??;
        self.cancel_worker_fetch_upload_connection(upload_key);
        let opened = loader
            .open_fetch_response_stream_async(NativeFetchRequest {
                document_url: &worker_url,
                href: &href,
                method,
                body: Some(NativeRequestBody::Bytes(body)),
                content_type,
                request_headers: headers,
                credentials,
                credentials_mode: Some(credentials_mode),
                referrer_url,
                referrer_policy,
                cors_mode,
                redirect_mode,
                cache_mode,
                timeout,
                max_response_bytes: None,
            })
            .await;
        Ok(opened)
    }

    pub(super) fn worker_fetch_opened_payload(
        &mut self,
        worker_id: u32,
        request_id: u32,
        opened: Result<NativeFetchResponseStream, NativeEngineError>,
    ) -> serde_json::Value {
        match opened {
            Ok(mut opened)
                if !opened.response.opaque
                    && !opened.response.opaque_redirect
                    && (opened.body.is_some() || opened.cached_body.is_some()) =>
            {
                let stream_key = (worker_id, request_id);
                if self
                    .worker_fetch_stream_connections
                    .contains_key(&stream_key)
                {
                    worker_fetch_response_payload(Err(NativeEngineError::Network {
                        operation: "native Worker fetch response stream".into(),
                        reason: "fetch response stream identifier is already active".into(),
                    }))
                } else if self.worker_fetch_stream_connections.len()
                    >= MAX_NATIVE_WORKER_FETCH_STREAMS
                {
                    worker_fetch_response_payload(Err(NativeEngineError::limit(
                        "native Worker fetch response streams",
                        MAX_NATIVE_WORKER_FETCH_STREAMS,
                        self.worker_fetch_stream_connections.len().saturating_add(1),
                    )))
                } else {
                    let response = opened.response;
                    let stream = if let Some(body) = opened.body.take() {
                        spawn_native_fetch_stream(body, opened.max_response_bytes)
                    } else {
                        spawn_native_fetch_bytes_stream(
                            opened
                                .cached_body
                                .take()
                                .expect("Worker fetch stream cached body is present"),
                            opened.max_response_bytes,
                        )
                    };
                    self.worker_fetch_stream_connections
                        .insert(stream_key, stream);
                    worker_fetch_response_stream_payload(response, request_id)
                }
            }
            Ok(mut opened) => {
                if let Some(body) = opened.cached_body.take() {
                    opened.response.body = body;
                }
                worker_fetch_response_payload(Ok(opened.response))
            }
            Err(error) => worker_fetch_response_payload(Err(error)),
        }
    }

    pub(super) async fn resolve_worker_fetch(
        &mut self,
        worker_id: u32,
        command: NativeScriptCommand,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let NativeScriptCommand::Fetch {
            request_id,
            worker_id: Some(command_worker_id),
            href,
            credentials,
            credentials_mode,
            referrer: _,
            referrer_url,
            referrer_policy,
            method,
            headers,
            body,
            body_base64,
            content_type,
            mode,
            redirect,
            cache,
            timeout_ms,
            upload_stream_id,
            destination,
            module_referrer,
            module_type,
        } = command
        else {
            return Err(NativeEngineError::invalid(
                "native Worker fetch command",
                "command was not a worker fetch",
            ));
        };
        if command_worker_id != worker_id {
            return Err(NativeEngineError::invalid(
                "native Worker fetch command",
                "worker id does not match the owning worker",
            ));
        }
        let credentials_mode = credentials_mode
            .as_deref()
            .map(NativeFetchCredentialsMode::parse)
            .transpose()?
            .unwrap_or(if credentials {
                NativeFetchCredentialsMode::Include
            } else {
                NativeFetchCredentialsMode::Omit
            });
        let module_destination = match destination.as_deref() {
            None | Some("") if module_referrer.is_none() => false,
            Some("module") if module_referrer.is_some() => true,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker fetch destination",
                    "module destination requires a referrer; other destinations are unsupported",
                ));
            }
        };
        if (!module_destination && module_type.is_some())
            || module_type == Some(NativeModuleResourceType::Unsupported)
        {
            return Err(NativeEngineError::invalid(
                "native Worker dynamic module type",
                "must be supported and present only for module-destination requests",
            ));
        }
        if module_destination
            && (!credentials
                || method != "GET"
                || !headers.is_empty()
                || body.is_some()
                || body_base64.is_some()
                || content_type.is_some()
                || mode.as_deref() != Some("cors")
                || redirect.as_deref() != Some("follow")
                || cache.as_deref() != Some("default")
                || timeout_ms.is_some()
                || upload_stream_id.is_some())
        {
            return Err(NativeEngineError::invalid(
                "native Worker dynamic module request",
                "must be a credentialed bodyless CORS GET with default cache and follow redirects",
            ));
        }
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native Worker fetch request id",
                "must be positive",
            ));
        }
        if timeout_ms.is_some_and(|value| value > MAX_NATIVE_XHR_TIMEOUT_MS) {
            return Err(NativeEngineError::invalid(
                "native Worker fetch timeout",
                "must not exceed the native XHR timeout limit",
            ));
        }
        let method = NativeFetchMethod::from_fetch_method(&method)?;
        let cors_mode = match mode.as_deref().unwrap_or("cors") {
            "cors" => NativeCorsMode::Cors,
            "no-cors" => NativeCorsMode::NoCors,
            "same-origin" => NativeCorsMode::SameOrigin,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker fetch mode",
                    "must be cors, no-cors, or same-origin",
                ));
            }
        };
        let redirect_mode = match redirect.as_deref().unwrap_or("follow") {
            "follow" => NativeFetchRedirectMode::Follow,
            "error" => NativeFetchRedirectMode::Error,
            "manual" => NativeFetchRedirectMode::Manual,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker fetch redirect mode",
                    "must be follow, error, or manual",
                ));
            }
        };
        let cache_mode = NativeFetchCacheMode::from_option(cache.as_deref())?;
        let body = if let Some(upload_stream_id) = upload_stream_id {
            if upload_stream_id != request_id {
                return Err(NativeEngineError::invalid(
                    "Worker fetch request upload stream",
                    "upload stream identifier must match its fetch request",
                ));
            }
            if body.is_some() || body_base64.is_some() {
                return Err(NativeEngineError::invalid(
                    "Worker fetch request upload stream",
                    "streaming fetch requests must not also carry a buffered body",
                ));
            }
            None
        } else {
            match body_base64 {
                Some(encoded) => Some(NativeRequestBody::Bytes(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_| {
                            NativeEngineError::invalid(
                                "native Worker fetch binary body",
                                "must be valid base64",
                            )
                        })?,
                )),
                None => body.map(NativeRequestBody::Text),
            }
        };
        let worker_url = {
            let worker = self.workers.get(&worker_id).ok_or_else(|| {
                NativeEngineError::invalid("native Worker fetch", "worker no longer exists")
            })?;
            worker.url.clone()
        };
        let payload = if module_destination {
            let module_referrer = module_referrer.as_deref().unwrap_or_default();
            let module_referrer_policy = referrer_policy
                .as_deref()
                .map(NativeFetchReferrerPolicy::parse)
                .transpose()?;
            match self
                .load_dynamic_worker_module(
                    NativeDynamicWorkerModuleRequest {
                        worker_id,
                        request_id,
                        worker_url: &worker_url,
                        module_referrer,
                        specifier: &href,
                        module_type,
                        referrer_policy: module_referrer_policy,
                    },
                    loader,
                    parent_fetch_broker.as_deref_mut(),
                )
                .await
            {
                Ok(module_key) => serde_json::json!({
                    "dynamicModuleImport": {"moduleKey": module_key},
                }),
                Err(_) => serde_json::json!({
                    "dynamicModuleImport": {"error": "Failed to load dynamically imported module"},
                }),
            }
        } else if let Some(upload_stream_id) = upload_stream_id {
            if !self.stream_worker_fetches {
                worker_fetch_response_payload(Err(NativeEngineError::UnsupportedUrl {
                    reason: "streaming Worker request bodies require the native stream host".into(),
                }))
            } else {
                let opened = self
                    .open_worker_fetch_upload(
                        NativeWorkerFetchUploadRequest {
                            worker_id,
                            request_id: upload_stream_id,
                            worker_url: worker_url.clone(),
                            href: href.clone(),
                            method,
                            headers,
                            content_type,
                            credentials,
                            credentials_mode,
                            referrer_url,
                            referrer_policy,
                            cors_mode,
                            redirect_mode,
                            cache_mode,
                            timeout: timeout_ms
                                .map(|value| Duration::from_millis(u64::from(value))),
                        },
                        loader,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                match opened {
                    Ok(opened) => {
                        self.worker_fetch_opened_payload(worker_id, request_id, Ok(opened))
                    }
                    Err(error) => worker_fetch_response_payload(Err(error)),
                }
            }
        } else {
            let request = NativeFetchRequest {
                document_url: &worker_url,
                href: &href,
                method,
                body,
                content_type,
                request_headers: headers,
                credentials,
                credentials_mode: Some(credentials_mode),
                referrer_url,
                referrer_policy,
                cors_mode,
                redirect_mode,
                cache_mode,
                timeout: timeout_ms.map(|value| Duration::from_millis(u64::from(value))),
                max_response_bytes: None,
            };
            if let Some(parent_fetch_broker) = parent_fetch_broker {
                let response = parent_fetch_broker.fetch(request_id, &request).await?.0;
                if self.stream_worker_fetches {
                    match response {
                        Ok(mut response) => {
                            let cached_body = Some(std::mem::take(&mut response.body));
                            self.worker_fetch_opened_payload(
                                worker_id,
                                request_id,
                                Ok(NativeFetchResponseStream {
                                    response,
                                    body: None,
                                    cached_body,
                                    max_response_bytes: MAX_NATIVE_FETCH_STREAM_BODY_BYTES,
                                }),
                            )
                        }
                        Err(error) => worker_fetch_response_payload(Err(error)),
                    }
                } else {
                    worker_fetch_response_payload(response)
                }
            } else if is_parent_owned_network_target(&worker_url, &href)
                && !self.parent_loader_owns_network
            {
                worker_fetch_response_payload(Err(missing_parent_network_authority(
                    "Worker Fetch request",
                )))
            } else if self.parent_loader_owns_network {
                worker_fetch_response_payload(
                    loader.fetch_request_with_headers_async(request).await,
                )
            } else if self.stream_worker_fetches {
                self.worker_fetch_opened_payload(
                    worker_id,
                    request_id,
                    loader.open_fetch_response_stream_async(request).await,
                )
            } else {
                worker_fetch_response_payload(
                    loader.fetch_request_with_headers_async(request).await,
                )
            }
        };
        self.evaluate_worker_with_loader(worker_id, loader, |worker| {
            worker.evaluate_turn_with_event(
                worker_id,
                NativeWorkerDispatch::Fetch {
                    request_id,
                    payload: &payload,
                },
            )
        })
    }
}
