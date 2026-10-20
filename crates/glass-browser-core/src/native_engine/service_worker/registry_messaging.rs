use super::*;

impl NativeServiceWorkerRegistry {
    pub(crate) fn clear_page_message_port_routes(&mut self) {
        self.pending_message_port_messages.clear();
        self.pending_client_messages.clear();
        self.message_port_routes.clear();
    }

    pub(crate) fn next_timer_delay_ms(&self) -> Result<Option<u64>, NativeEngineError> {
        let mut earliest = None;
        for worker in self
            .registrations
            .values()
            .chain(self.waiting_workers.values())
        {
            if let Some(delay) = worker.runtime.next_worker_timer_delay_ms()? {
                earliest = Some(earliest.map_or(delay, |current: u64| current.min(delay)));
            }
        }
        Ok(earliest)
    }

    /// Admit at most one due Service Worker timer turn at a page host
    /// boundary. Active and waiting workers share a rotating worker-id cursor
    /// so one continuously-ready registration cannot monopolize delivery to
    /// the page. The resulting host commands remain in the same bounded
    /// client/MessagePort queues as lifecycle and message events.
    #[cfg(test)]
    pub(crate) async fn run_due_timers(
        &mut self,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.run_due_timers_inner(loader, None).await
    }

    pub(crate) async fn run_due_timers_with_parent_fetch_broker(
        &mut self,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<(), NativeEngineError> {
        self.run_due_timers_inner(loader, Some(parent_fetch_broker))
            .await
    }

    pub(super) async fn run_due_timers_inner<'broker>(
        &mut self,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
    ) -> Result<(), NativeEngineError> {
        let mut workers = self
            .registrations
            .iter()
            .map(|(scope, worker)| (worker.id, false, scope.clone()))
            .chain(
                self.waiting_workers
                    .iter()
                    .map(|(scope, worker)| (worker.id, true, scope.clone())),
            )
            .collect::<Vec<_>>();
        workers.sort_unstable_by_key(|(worker_id, _, _)| *worker_id);

        let mut selected = None;
        let after_cursor = workers
            .iter()
            .filter(|(worker_id, _, _)| *worker_id > self.next_timer_worker_id);
        let before_cursor = workers
            .iter()
            .filter(|(worker_id, _, _)| *worker_id <= self.next_timer_worker_id);
        for (worker_id, waiting, scope) in after_cursor.chain(before_cursor) {
            let worker = if *waiting {
                self.waiting_workers.get(scope)
            } else {
                self.registrations.get(scope)
            };
            if let Some(worker) = worker
                && worker.runtime.next_worker_timer_delay_ms()? == Some(0)
            {
                selected = Some((*worker_id, *waiting, scope.clone()));
                break;
            }
        }
        let Some((worker_id, waiting, scope)) = selected else {
            return Ok(());
        };
        self.next_timer_worker_id = worker_id;

        let evaluation = if waiting {
            let worker = self.waiting_workers.get(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            worker.runtime.evaluate_service_worker_timers(
                worker.id,
                &worker.script_url,
                worker.is_module,
            )?
        } else {
            let worker = self.registrations.get(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            worker.runtime.evaluate_service_worker_timers(
                worker.id,
                &worker.script_url,
                worker.is_module,
            )?
        };

        let client_messages = if waiting {
            let worker = self.waiting_workers.get_mut(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            settle_service_worker_cache_event_with_parent_fetch_broker(
                worker,
                evaluation,
                loader,
                &mut self.cache_state,
                None,
                &mut self.pending_open_windows,
                NativeServiceWorkerFetchAuthority {
                    parent_fetch_broker: parent_fetch_broker.as_deref_mut(),
                    allow_captured_load_fetch: false,
                },
            )
            .await?
        } else {
            let worker = self.registrations.get_mut(&scope).ok_or_else(|| {
                NativeEngineError::invalid("service worker timer", "worker vanished")
            })?;
            settle_service_worker_cache_event_with_parent_fetch_broker(
                worker,
                evaluation,
                loader,
                &mut self.cache_state,
                None,
                &mut self.pending_open_windows,
                NativeServiceWorkerFetchAuthority {
                    parent_fetch_broker,
                    allow_captured_load_fetch: false,
                },
            )
            .await?
        };
        self.enqueue_client_messages(client_messages)?;

        let message_port_commands = if waiting {
            self.waiting_workers
                .get(&scope)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default()
        } else {
            self.registrations
                .get(&scope)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default()
        };
        self.collect_message_port_commands(worker_id, message_port_commands)
    }

    pub(crate) fn take_message_port_messages(&mut self) -> Vec<NativeMessagePortPageMessage> {
        self.pending_message_port_messages.drain(..).collect()
    }

    pub(crate) fn take_client_messages(&mut self) -> Vec<NativeServiceWorkerClientMessage> {
        self.take_client_messages_matching(true)
    }

    pub(crate) fn take_external_client_messages(
        &mut self,
    ) -> Vec<NativeServiceWorkerClientMessage> {
        self.take_client_messages_matching(false)
    }

    pub(super) fn take_client_messages_matching(
        &mut self,
        current: bool,
    ) -> Vec<NativeServiceWorkerClientMessage> {
        let current_client_id = self.current_client_id.clone();
        let mut selected = Vec::new();
        let mut retained = VecDeque::new();
        for message in self.pending_client_messages.drain(..) {
            if (message.client_id == current_client_id) == current {
                selected.push(message);
            } else {
                retained.push_back(message);
            }
        }
        self.pending_client_messages = retained;
        selected
    }

    pub(crate) fn take_open_windows(&mut self) -> Vec<NativeServiceWorkerOpenWindowRequest> {
        self.pending_open_windows
            .iter()
            .filter(|&request| {
                self.announced_open_windows
                    .insert((request.worker_id, request.request_id))
            })
            .cloned()
            .collect()
    }

    pub(crate) fn take_completed_fetch(&mut self) -> Option<NativeServiceWorkerFetchCompletion> {
        self.completed_fetches.pop_front()
    }

    pub(super) fn enqueue_client_messages(
        &mut self,
        messages: Vec<NativeServiceWorkerClientMessage>,
    ) -> Result<(), NativeEngineError> {
        if messages.len() > MAX_NATIVE_WORKER_MESSAGES
            || self
                .pending_client_messages
                .len()
                .saturating_add(messages.len())
                > MAX_NATIVE_WORKER_MESSAGES
        {
            return Err(NativeEngineError::limit(
                "native service-worker client messages",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_client_messages
                    .len()
                    .saturating_add(messages.len()),
            ));
        }
        for message in &messages {
            self.register_worker_transfers(message.worker_id, &message.transfer_ports)?;
        }
        self.pending_client_messages.extend(messages);
        Ok(())
    }

    pub(crate) fn unregister(&mut self, scope: &str) -> bool {
        let removed_profile = self
            .registration_profiles
            .iter()
            .any(|profile| profile.scope == scope);
        self.registration_profiles
            .retain(|profile| profile.scope != scope);
        let removed_active = self.registrations.remove(scope);
        let had_active = removed_active.is_some();
        if let Some(worker) = removed_active {
            self.remove_worker_routes(worker.id);
        }
        let removed_waiting = self.waiting_workers.remove(scope);
        let had_waiting = removed_waiting.is_some();
        if let Some(worker) = removed_waiting {
            self.remove_worker_routes(worker.id);
        }
        if self.current_client_scope.as_deref() == Some(scope) {
            self.current_client_scope = None;
        }
        let removed = removed_profile || had_active || had_waiting;
        if removed {
            self.record_registration_change(scope);
        }
        removed
    }

    pub(crate) async fn post_message(
        &mut self,
        loader: &mut NativeResourceLoader,
        scope: &str,
        source_origin: &str,
        data: &Value,
        transfer_ports: &[NativeMessagePortTransfer],
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        let Some(worker_id) = self.registrations.get(scope).map(|worker| worker.id) else {
            return Ok(());
        };
        validate_url_text("native service worker message origin", source_origin)?;
        let current_client_url = self.current_client_url.as_deref().ok_or_else(|| {
            NativeEngineError::invalid(
                "native service worker message source",
                "current client is unavailable",
            )
        })?;
        let current_client = Url::parse(without_fragment(current_client_url)).map_err(|_| {
            NativeEngineError::invalid(
                "native service worker message source",
                "current client URL is invalid",
            )
        })?;
        let source_origin_state = NativeOrigin::from_url(&current_client)?;
        let scope_url = Url::parse(without_fragment(scope)).map_err(|_| {
            NativeEngineError::invalid("native service worker scope", "URL is invalid")
        })?;
        if source_origin_state != NativeOrigin::from_url(&scope_url)?
            || source_origin_state.serialized() != source_origin
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "service worker message source must match the current same-origin client"
                    .into(),
            });
        }
        validate_message_port_transfers(transfer_ports)?;
        let clients = self.client_states_for_worker(scope)?;
        let source = clients
            .iter()
            .find(|client| {
                client.get("clientId").and_then(Value::as_str) == Some(&self.current_client_id)
            })
            .cloned()
            .filter(|client| {
                client.get("clientUrl").and_then(Value::as_str)
                    == Some(without_fragment(current_client_url))
            })
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker message source",
                    "current client is missing from the validated worker projection",
                )
            })?;
        self.register_page_transfers(worker_id, transfer_ports)?;
        let evaluation = {
            let worker = self
                .registrations
                .get(scope)
                .ok_or_else(|| NativeEngineError::invalid("service worker scope", "not found"))?;
            worker.runtime.set_service_worker_clients(clients);
            worker
                .runtime
                .dispatch_service_worker_message(NativeServiceWorkerMessageEvent {
                    worker_id: worker.id,
                    worker_url: &worker.script_url,
                    data,
                    transfer_ports,
                    source: &source,
                    source_origin,
                    is_module: worker.is_module,
                })
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) => {
                self.remove_transfer_routes(transfer_ports);
                return Err(error);
            }
        };
        let message_port_commands = self
            .registrations
            .get(scope)
            .map(|worker| worker.runtime.take_message_port_commands())
            .unwrap_or_default();
        if let Err(error) = self.collect_message_port_commands(worker_id, message_port_commands) {
            self.remove_transfer_routes(transfer_ports);
            return Err(error);
        }
        let client_messages = match settle_service_worker_cache_event_with_parent_fetch_broker(
            self.registrations
                .get_mut(scope)
                .expect("service worker registration was retained"),
            evaluation,
            loader,
            &mut self.cache_state,
            None,
            &mut self.pending_open_windows,
            NativeServiceWorkerFetchAuthority {
                parent_fetch_broker,
                allow_captured_load_fetch: false,
            },
        )
        .await
        {
            Ok(messages) => messages,
            Err(error) => {
                self.remove_transfer_routes(transfer_ports);
                return Err(error);
            }
        };
        if let Err(error) = self.enqueue_client_messages(client_messages) {
            self.remove_transfer_routes(transfer_ports);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn resolve_open_window(
        &mut self,
        loader: &mut NativeResourceLoader,
        worker_id: u32,
        request_id: u32,
        payload: &Value,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<bool, NativeEngineError> {
        if worker_id == 0 || request_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service worker openWindow resolution",
                "request and worker ids must be positive",
            ));
        }
        let request_index = self
            .pending_open_windows
            .iter()
            .position(|request| request.worker_id == worker_id && request.request_id == request_id)
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker openWindow resolution",
                    "request is unknown or was already resolved",
                )
            })?;
        let request = self
            .pending_open_windows
            .remove(request_index)
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker openWindow resolution",
                    "request queue changed while resolving",
                )
            })?;
        self.announced_open_windows
            .remove(&(request.worker_id, request.request_id));
        let scope = self
            .registrations
            .iter()
            .find_map(|(scope, worker)| (worker.id == request.worker_id).then_some(scope.clone()))
            .or_else(|| {
                self.waiting_workers.iter().find_map(|(scope, worker)| {
                    (worker.id == request.worker_id).then_some(scope.clone())
                })
            })
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native service worker openWindow resolution",
                    "worker is no longer registered",
                )
            })?;
        let current_client_id =
            (!self.current_client_id.is_empty()).then(|| self.current_client_id.clone());
        let continuation = self.pending_fetches.remove(&(worker_id, request_id));
        let (evaluation, worker_is_waiting) = if let Some(worker) = self.registrations.get(&scope) {
            (
                worker.runtime.resolve_service_worker_open_window(
                    worker.id,
                    &worker.script_url,
                    request.request_id,
                    payload,
                    worker.is_module,
                )?,
                false,
            )
        } else if let Some(worker) = self.waiting_workers.get(&scope) {
            (
                worker.runtime.resolve_service_worker_open_window(
                    worker.id,
                    &worker.script_url,
                    request.request_id,
                    payload,
                    worker.is_module,
                )?,
                true,
            )
        } else {
            return Err(NativeEngineError::invalid(
                "native service worker openWindow resolution",
                "worker is no longer registered",
            ));
        };
        if let Some(continuation) = continuation {
            let fallback_url = continuation.fallback_url.clone();
            let mut pending = continuation.pending;
            pending.extend(evaluation.commands);
            let evaluation = NativeScriptEvaluation {
                value: continuation.value,
                commands: pending.into_iter().collect(),
                top_level_await_pending: continuation.awaiting
                    || evaluation.top_level_await_pending,
                worker_script_error: None,
            };
            let settlement = if worker_is_waiting {
                let worker = self
                    .waiting_workers
                    .get_mut(&scope)
                    .expect("waiting service worker was retained");
                settle_service_worker_fetch_with_parent_fetch_broker(
                    worker,
                    loader,
                    evaluation,
                    &mut self.cache_state,
                    NativeServiceWorkerFetchSettlementContext {
                        current_client_id: current_client_id.as_deref().unwrap_or_default(),
                        fallback_url: &fallback_url,
                        pending_open_windows: &mut self.pending_open_windows,
                        lifetime_fetch_tasks: &mut self.lifetime_fetch_tasks,
                        lifetime_fetch_abort_handles: &mut self.lifetime_fetch_abort_handles,
                        authority: NativeServiceWorkerFetchAuthority {
                            parent_fetch_broker: Some(&mut *parent_fetch_broker),
                            allow_captured_load_fetch: false,
                        },
                    },
                )
                .await?
            } else {
                let worker = self
                    .registrations
                    .get_mut(&scope)
                    .expect("active service worker was retained");
                settle_service_worker_fetch_with_parent_fetch_broker(
                    worker,
                    loader,
                    evaluation,
                    &mut self.cache_state,
                    NativeServiceWorkerFetchSettlementContext {
                        current_client_id: current_client_id.as_deref().unwrap_or_default(),
                        fallback_url: &fallback_url,
                        pending_open_windows: &mut self.pending_open_windows,
                        lifetime_fetch_tasks: &mut self.lifetime_fetch_tasks,
                        lifetime_fetch_abort_handles: &mut self.lifetime_fetch_abort_handles,
                        authority: NativeServiceWorkerFetchAuthority {
                            parent_fetch_broker: Some(&mut *parent_fetch_broker),
                            allow_captured_load_fetch: false,
                        },
                    },
                )
                .await?
            };
            match settlement {
                NativeServiceWorkerFetchSettlement::Complete {
                    value,
                    client_messages,
                } => {
                    self.enqueue_client_messages(client_messages)?;
                    let response = if value.get("handled").and_then(Value::as_bool) == Some(true) {
                        let response =
                            value
                                .get("response")
                                .ok_or_else(|| NativeEngineError::Worker {
                                    operation: "decode service worker response".into(),
                                    reason: "service worker returned no response envelope".into(),
                                })?;
                        Some(decode_service_worker_response(response, &fallback_url)?)
                    } else {
                        None
                    };
                    if self.completed_fetches.len() >= MAX_NATIVE_EFFECTS {
                        return Err(NativeEngineError::limit(
                            "native completed service worker fetches",
                            MAX_NATIVE_EFFECTS,
                            self.completed_fetches.len().saturating_add(1),
                        ));
                    }
                    self.completed_fetches
                        .push_back(NativeServiceWorkerFetchCompletion {
                            worker_id,
                            response,
                        });
                    let message_port_commands = if worker_is_waiting {
                        self.waiting_workers
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    } else {
                        self.registrations
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    };
                    self.collect_message_port_commands(worker_id, message_port_commands)?;
                    Ok(true)
                }
                NativeServiceWorkerFetchSettlement::Suspended {
                    continuation,
                    client_messages,
                } => {
                    self.enqueue_client_messages(client_messages)?;
                    let key = (continuation.worker_id, continuation.open_window_request_id);
                    if self.pending_fetches.insert(key, continuation).is_some() {
                        return Err(NativeEngineError::Worker {
                            operation: "service worker fetch event".into(),
                            reason: "service worker fetch continuation identifier collided".into(),
                        });
                    }
                    let message_port_commands = if worker_is_waiting {
                        self.waiting_workers
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    } else {
                        self.registrations
                            .get(&scope)
                            .map(|worker| worker.runtime.take_message_port_commands())
                            .unwrap_or_default()
                    };
                    self.collect_message_port_commands(worker_id, message_port_commands)?;
                    Ok(false)
                }
            }
        } else {
            let client_messages = if worker_is_waiting {
                let worker = self
                    .waiting_workers
                    .get_mut(&scope)
                    .expect("waiting service worker was retained");
                settle_service_worker_cache_event(
                    worker,
                    evaluation,
                    loader,
                    &mut self.cache_state,
                    current_client_id.as_deref(),
                    &mut self.pending_open_windows,
                )
                .await?
            } else {
                let worker = self
                    .registrations
                    .get_mut(&scope)
                    .expect("active service worker was retained");
                settle_service_worker_cache_event(
                    worker,
                    evaluation,
                    loader,
                    &mut self.cache_state,
                    current_client_id.as_deref(),
                    &mut self.pending_open_windows,
                )
                .await?
            };
            self.enqueue_client_messages(client_messages)?;
            let message_port_commands = if worker_is_waiting {
                self.waiting_workers
                    .get(&scope)
                    .map(|worker| worker.runtime.take_message_port_commands())
                    .unwrap_or_default()
            } else {
                self.registrations
                    .get(&scope)
                    .map(|worker| worker.runtime.take_message_port_commands())
                    .unwrap_or_default()
            };
            self.collect_message_port_commands(worker_id, message_port_commands)?;
            Ok(false)
        }
    }

    pub(crate) async fn apply_page_message_port_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        if commands.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native page MessagePort commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len(),
            ));
        }
        for command in commands {
            let (bridge_key, data, transfer_ports, object_urls) = match command {
                NativeScriptCommand::MessagePortClose {
                    bridge_key,
                    worker_id: None,
                } => {
                    validate_service_worker_message_port_bridge_key(&bridge_key)?;
                    let Some(worker_id) = self.message_port_routes.get(&bridge_key).copied() else {
                        continue;
                    };
                    let Some(scope) = self.registrations.iter().find_map(|(scope, worker)| {
                        (worker.id == worker_id).then_some(scope.clone())
                    }) else {
                        self.retire_message_port_route(&bridge_key);
                        continue;
                    };
                    if self.retire_message_port_route(&bridge_key).is_none() {
                        continue;
                    }
                    let evaluation = {
                        let worker = self.registrations.get(&scope).ok_or_else(|| {
                            NativeEngineError::invalid("service worker scope", "not found")
                        })?;
                        worker.runtime.dispatch_service_worker_message_port_close(
                            worker.id,
                            &worker.script_url,
                            &bridge_key,
                            worker.is_module,
                        )
                    }?;
                    let message_port_commands = self
                        .registrations
                        .get(&scope)
                        .map(|worker| worker.runtime.take_message_port_commands())
                        .unwrap_or_default();
                    self.collect_message_port_commands(worker_id, message_port_commands)?;
                    let client_messages =
                        settle_service_worker_cache_event_with_parent_fetch_broker(
                            self.registrations
                                .get_mut(&scope)
                                .expect("service worker registration was retained"),
                            evaluation,
                            loader,
                            &mut self.cache_state,
                            None,
                            &mut self.pending_open_windows,
                            NativeServiceWorkerFetchAuthority {
                                parent_fetch_broker: parent_fetch_broker.as_deref_mut(),
                                allow_captured_load_fetch: false,
                            },
                        )
                        .await?;
                    self.enqueue_client_messages(client_messages)?;
                    continue;
                }
                NativeScriptCommand::MessagePortPostMessage {
                    bridge_key,
                    data,
                    worker_id: None,
                    transfer_ports,
                    object_urls,
                } => (bridge_key, data, transfer_ports, object_urls),
                _ => {
                    return Err(NativeEngineError::invalid(
                        "native page MessagePort command",
                        "command did not originate from the page realm",
                    ));
                }
            };
            let Some(worker_id) = self.message_port_routes.get(&bridge_key).copied() else {
                continue;
            };
            let Some(scope) = self
                .registrations
                .iter()
                .find_map(|(scope, worker)| (worker.id == worker_id).then_some(scope.clone()))
            else {
                self.message_port_routes.remove(&bridge_key);
                continue;
            };
            self.register_page_transfers(worker_id, &transfer_ports)?;
            let evaluation = {
                let worker = self.registrations.get(&scope).ok_or_else(|| {
                    NativeEngineError::invalid("service worker scope", "not found")
                })?;
                worker.runtime.dispatch_service_worker_message_port(
                    worker.id,
                    &worker.script_url,
                    &bridge_key,
                    &data,
                    &transfer_ports,
                    &object_urls,
                )
            };
            let evaluation = match evaluation {
                Ok(evaluation) => evaluation,
                Err(error) => {
                    self.remove_transfer_routes(&transfer_ports);
                    return Err(error);
                }
            };
            let message_port_commands = self
                .registrations
                .get(&scope)
                .map(|worker| worker.runtime.take_message_port_commands())
                .unwrap_or_default();
            if let Err(error) = self.collect_message_port_commands(worker_id, message_port_commands)
            {
                self.remove_transfer_routes(&transfer_ports);
                return Err(error);
            }
            let client_messages = match settle_service_worker_cache_event_with_parent_fetch_broker(
                self.registrations
                    .get_mut(&scope)
                    .expect("service worker registration was retained"),
                evaluation,
                loader,
                &mut self.cache_state,
                None,
                &mut self.pending_open_windows,
                NativeServiceWorkerFetchAuthority {
                    parent_fetch_broker: parent_fetch_broker.as_deref_mut(),
                    allow_captured_load_fetch: false,
                },
            )
            .await
            {
                Ok(messages) => messages,
                Err(error) => {
                    self.remove_transfer_routes(&transfer_ports);
                    return Err(error);
                }
            };
            if let Err(error) = self.enqueue_client_messages(client_messages) {
                self.remove_transfer_routes(&transfer_ports);
                return Err(error);
            }
        }
        Ok(())
    }
}
