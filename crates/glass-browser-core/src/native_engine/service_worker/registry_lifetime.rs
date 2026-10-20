use super::*;

impl NativeServiceWorkerRegistry {
    pub(crate) fn has_lifetime_fetch_tasks(&self) -> bool {
        !self.lifetime_fetch_tasks.is_empty()
    }

    pub(crate) fn has_pending_parent_lifetime_fetches(&self) -> bool {
        !self.pending_parent_lifetime_fetches.is_empty()
    }

    pub(crate) async fn next_lifetime_fetch_task(
        &mut self,
    ) -> Result<Option<NativeServiceWorkerFetchTaskResult>, NativeEngineError> {
        loop {
            let Some(result) = self.lifetime_fetch_tasks.join_next().await else {
                return Ok(None);
            };
            match result {
                Ok(completion) => {
                    let (worker_id, request_id) = match &completion {
                        NativeServiceWorkerFetchTaskResult::Completed(completion) => {
                            (completion.worker_id, completion.request_id)
                        }
                        NativeServiceWorkerFetchTaskResult::ParentBrokered(task) => {
                            (task.worker_id, task.request_id)
                        }
                    };
                    self.lifetime_fetch_abort_handles
                        .remove(&(worker_id, request_id));
                    return Ok(Some(completion));
                }
                Err(error) if error.is_cancelled() => {
                    self.lifetime_fetch_abort_handles
                        .retain(|_, handle| !handle.is_finished());
                }
                Err(error) => {
                    self.lifetime_fetch_abort_handles
                        .retain(|_, handle| !handle.is_finished());
                    return Err(NativeEngineError::Worker {
                        operation: "service worker lifetime fetch".into(),
                        reason: format!("background fetch task failed: {error}"),
                    });
                }
            }
        }
    }

    pub(crate) fn queue_parent_brokered_lifetime_fetch(
        &mut self,
        task: NativeServiceWorkerFetchTaskResult,
    ) -> Result<(), NativeEngineError> {
        let NativeServiceWorkerFetchTaskResult::ParentBrokered(task) = task else {
            return Err(NativeEngineError::invalid(
                "Service Worker parent Fetch queue",
                "only parent-brokered Fetch tasks can be queued",
            ));
        };
        let task = *task;
        if self.pending_parent_lifetime_fetches.len() >= MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native pending Service Worker parent Fetch tasks",
                MAX_NATIVE_EFFECTS,
                self.pending_parent_lifetime_fetches.len().saturating_add(1),
            ));
        }
        if self.pending_parent_lifetime_fetches.iter().any(|pending| {
            (pending.worker_id, pending.request_id) == (task.worker_id, task.request_id)
        }) {
            return Err(NativeEngineError::invalid(
                "Service Worker parent Fetch queue",
                "request identifier is already queued",
            ));
        }
        self.pending_parent_lifetime_fetches.push_back(task);
        Ok(())
    }

    pub(crate) async fn run_next_parent_brokered_lifetime_fetch(
        &mut self,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<bool, NativeEngineError> {
        let Some(task) = self.pending_parent_lifetime_fetches.pop_front() else {
            return Ok(false);
        };
        let worker_url = self
            .registrations
            .values()
            .chain(self.waiting_workers.values())
            .find(|worker| worker.id == task.worker_id)
            .map(|worker| worker.script_url.clone());
        let Some(worker_url) = worker_url else {
            return Ok(true);
        };
        let request = task.request;
        let fetch_request = NativeFetchRequest {
            document_url: &worker_url,
            href: &request.href,
            method: request.method,
            body: request.body,
            content_type: request.content_type,
            request_headers: request.headers,
            credentials: request.credentials,
            credentials_mode: Some(request.credentials_mode),
            referrer_url: request.referrer_url,
            referrer_policy: request.referrer_policy,
            cors_mode: request.cors_mode,
            redirect_mode: request.redirect_mode,
            cache_mode: request.cache_mode,
            timeout: request.timeout,
            max_response_bytes: None,
        };
        let response = parent_fetch_broker
            .fetch(task.request_id, &fetch_request)
            .await?
            .0;
        let payload = match response {
            Ok(response) => service_worker_fetch_payload(response),
            Err(error) => json!({
                "error": error.to_string(),
                "timeout": false,
            }),
        };
        self.resolve_lifetime_fetch_task(
            NativeServiceWorkerFetchTaskCompletion {
                worker_id: task.worker_id,
                request_id: task.request_id,
                fallback_url: task.fallback_url,
                payload,
                loader: None,
            },
            loader,
            Some(parent_fetch_broker),
        )
        .await?;
        Ok(true)
    }

    pub(crate) async fn resolve_lifetime_fetch_task<'broker>(
        &mut self,
        completion: NativeServiceWorkerFetchTaskCompletion,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<(), NativeEngineError> {
        if let Some(task_loader) = completion.loader {
            loader.merge_fetch_task_state(task_loader)?;
        }
        let scope = self
            .registrations
            .iter()
            .find_map(|(scope, worker)| (worker.id == completion.worker_id).then(|| scope.clone()))
            .or_else(|| {
                self.waiting_workers.iter().find_map(|(scope, worker)| {
                    (worker.id == completion.worker_id).then(|| scope.clone())
                })
            });
        let Some(scope) = scope else {
            return Ok(());
        };
        let worker_is_waiting = !self
            .registrations
            .get(&scope)
            .is_some_and(|worker| worker.id == completion.worker_id);
        let current_client_id =
            (!self.current_client_id.is_empty()).then(|| self.current_client_id.clone());
        let (settlement, clients_claim_requested, message_port_commands) = if worker_is_waiting {
            let worker = self
                .waiting_workers
                .get_mut(&scope)
                .expect("waiting Service Worker was located above");
            let evaluation = worker.runtime.resolve_service_worker_fetch(
                worker.id,
                &worker.script_url,
                completion.request_id,
                &completion.payload,
                worker.is_module,
            )?;
            let settlement = settle_service_worker_fetch_with_parent_fetch_broker(
                worker,
                loader,
                evaluation,
                &mut self.cache_state,
                NativeServiceWorkerFetchSettlementContext {
                    current_client_id: current_client_id.as_deref().unwrap_or_default(),
                    fallback_url: &completion.fallback_url,
                    pending_open_windows: &mut self.pending_open_windows,
                    lifetime_fetch_tasks: &mut self.lifetime_fetch_tasks,
                    lifetime_fetch_abort_handles: &mut self.lifetime_fetch_abort_handles,
                    authority: NativeServiceWorkerFetchAuthority {
                        parent_fetch_broker: parent_fetch_broker.as_deref_mut(),
                        allow_captured_load_fetch: false,
                    },
                },
            )
            .await?;
            let message_port_commands = worker.runtime.take_message_port_commands();
            (
                settlement,
                worker.clients_claim_requested,
                message_port_commands,
            )
        } else {
            let worker = self
                .registrations
                .get_mut(&scope)
                .expect("active Service Worker was located above");
            let evaluation = worker.runtime.resolve_service_worker_fetch(
                worker.id,
                &worker.script_url,
                completion.request_id,
                &completion.payload,
                worker.is_module,
            )?;
            let settlement = settle_service_worker_fetch_with_parent_fetch_broker(
                worker,
                loader,
                evaluation,
                &mut self.cache_state,
                NativeServiceWorkerFetchSettlementContext {
                    current_client_id: current_client_id.as_deref().unwrap_or_default(),
                    fallback_url: &completion.fallback_url,
                    pending_open_windows: &mut self.pending_open_windows,
                    lifetime_fetch_tasks: &mut self.lifetime_fetch_tasks,
                    lifetime_fetch_abort_handles: &mut self.lifetime_fetch_abort_handles,
                    authority: NativeServiceWorkerFetchAuthority {
                        parent_fetch_broker,
                        allow_captured_load_fetch: false,
                    },
                },
            )
            .await?;
            let message_port_commands = worker.runtime.take_message_port_commands();
            (
                settlement,
                worker.clients_claim_requested,
                message_port_commands,
            )
        };
        if clients_claim_requested {
            self.claim_current_client(&scope)?;
        }
        self.collect_message_port_commands(completion.worker_id, message_port_commands)?;
        match settlement {
            NativeServiceWorkerFetchSettlement::Complete {
                client_messages, ..
            } => self.enqueue_client_messages(client_messages)?,
            NativeServiceWorkerFetchSettlement::Suspended {
                continuation,
                client_messages,
            } => {
                self.enqueue_client_messages(client_messages)?;
                let key = (continuation.worker_id, continuation.open_window_request_id);
                if self.pending_fetches.insert(key, continuation).is_some() {
                    return Err(NativeEngineError::Worker {
                        operation: "service worker lifetime fetch".into(),
                        reason: "fetch continuation identifier collided".into(),
                    });
                }
            }
        }
        Ok(())
    }
}
