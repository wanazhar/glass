use super::*;

impl NativeWorkerRegistry {
    pub async fn apply_page_message_port_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.apply_page_message_port_commands_inner(commands, loader, false, None)
            .await
    }

    pub(crate) async fn apply_page_message_port_commands_with_parent_fetch_broker(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<(), NativeEngineError> {
        self.apply_page_message_port_commands_inner(
            commands,
            loader,
            false,
            Some(parent_fetch_broker),
        )
        .await
    }

    pub(crate) async fn apply_page_message_port_commands_with_external_shared_workers(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        self.apply_page_message_port_commands_inner(commands, loader, true, None)
            .await
    }

    pub(crate) async fn apply_page_message_port_commands_with_external_shared_workers_and_parent_fetch_broker(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        parent_fetch_broker: &mut NativeContentFetchBroker<'_>,
    ) -> Result<(), NativeEngineError> {
        self.apply_page_message_port_commands_inner(
            commands,
            loader,
            true,
            Some(parent_fetch_broker),
        )
        .await
    }

    async fn apply_page_message_port_commands_inner<'broker>(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        loader: &mut NativeResourceLoader,
        forward_unrouted_closes: bool,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'broker>>,
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
                    validate_url_text("native MessagePort bridge key", &bridge_key)?;
                    if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
                        return Err(NativeEngineError::limit(
                            "native MessagePort bridge key",
                            crate::limits::MAX_BACKEND_ID_BYTES,
                            bridge_key.len(),
                        ));
                    }
                    let Some(route) = self.message_port_routes.get(&bridge_key).cloned() else {
                        if forward_unrouted_closes {
                            let command = NativePageMessagePortCommand {
                                bridge_key,
                                data: serde_json::Value::Null,
                                close: true,
                                transfer_ports: Vec::new(),
                                object_urls: Vec::new(),
                                source_context_id: String::new(),
                                source_frame_id: String::new(),
                            };
                            if self.pending_page_message_port_commands.len()
                                >= MAX_NATIVE_WORKER_MESSAGES
                            {
                                return Err(NativeEngineError::limit(
                                    "native pending page MessagePort commands",
                                    MAX_NATIVE_WORKER_MESSAGES,
                                    self.pending_page_message_port_commands
                                        .len()
                                        .saturating_add(1),
                                ));
                            }
                            self.pending_page_message_port_commands.push_back(command);
                        }
                        continue;
                    };
                    if self.retire_message_port_route(&bridge_key).is_none() {
                        continue;
                    }
                    let worker_id = route.worker_id;
                    let evaluation =
                        self.evaluate_worker_with_loader(worker_id, loader, |worker| {
                            worker.evaluate_turn_with_event(
                                worker_id,
                                NativeWorkerDispatch::MessagePortClose {
                                    bridge_key: &bridge_key,
                                },
                            )
                        });
                    match evaluation {
                        Ok(evaluation) => {
                            self.collect_worker_evaluation_with_parent_fetch_broker(
                                worker_id,
                                evaluation,
                                loader,
                                parent_fetch_broker.as_deref_mut(),
                            )
                            .await?;
                        }
                        Err(error) => {
                            let worker_url = self
                                .workers
                                .get(&worker_id)
                                .map(|worker| worker.url.clone())
                                .unwrap_or_default();
                            self.workers.remove(&worker_id);
                            self.remove_worker_routes(worker_id);
                            self.queue_error(worker_id, &worker_url, &error.to_string())?;
                        }
                    }
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
            let Some(route) = self.message_port_routes.get(&bridge_key).cloned() else {
                let command = NativePageMessagePortCommand {
                    bridge_key,
                    data,
                    close: false,
                    transfer_ports,
                    object_urls,
                    source_context_id: String::new(),
                    source_frame_id: String::new(),
                };
                validate_page_message_port_command(&command)?;
                if self.pending_page_message_port_commands.len() >= MAX_NATIVE_WORKER_MESSAGES {
                    return Err(NativeEngineError::limit(
                        "native pending page MessagePort commands",
                        MAX_NATIVE_WORKER_MESSAGES,
                        self.pending_page_message_port_commands
                            .len()
                            .saturating_add(1),
                    ));
                }
                self.pending_page_message_port_commands.push_back(command);
                continue;
            };
            self.register_page_transfers(route.worker_id, &transfer_ports)?;
            validate_native_object_url_transfers(&object_urls)?;
            validate_native_message_payload(
                &serde_json::json!({
                    "data": &data,
                    "transfer_ports": &transfer_ports,
                    "object_urls": &object_urls,
                }),
                "native page MessagePort event",
            )?;
            let worker_id = route.worker_id;
            let evaluation = self.evaluate_worker_with_loader(worker_id, loader, |worker| {
                worker.evaluate_turn_with_event(
                    worker_id,
                    NativeWorkerDispatch::MessagePort {
                        bridge_key: &bridge_key,
                        data: &data,
                        transfer_ports: &transfer_ports,
                        object_urls: &object_urls,
                    },
                )
            });
            match evaluation {
                Ok(evaluation) => {
                    self.collect_worker_evaluation_with_parent_fetch_broker(
                        worker_id,
                        evaluation,
                        loader,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                }
                Err(error) => {
                    let worker_url = self
                        .workers
                        .get(&worker_id)
                        .map(|worker| worker.url.clone())
                        .unwrap_or_default();
                    self.workers.remove(&worker_id);
                    self.remove_worker_routes(worker_id);
                    self.queue_error(worker_id, &worker_url, &error.to_string())?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn register_page_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        self.register_transfers(worker_id, transfers)
    }

    pub(super) fn register_worker_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        self.register_transfers(worker_id, transfers)
    }

    pub(super) fn validate_transfer_registration(
        &self,
        transfers: &[NativeMessagePortTransfer],
        additional_events: usize,
    ) -> Result<(), NativeEngineError> {
        if transfers.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native MessagePort transfers",
                MAX_NATIVE_WORKER_MESSAGES,
                transfers.len(),
            ));
        }
        validate_message_port_transfers(transfers)?;
        let mut new_routes = 0usize;
        for transfer in transfers {
            if self.message_port_routes.contains_key(&transfer.bridge_key) {
                return Err(NativeEngineError::invalid(
                    "native MessagePort transfer",
                    "bridge key was already transferred",
                ));
            }
            new_routes = new_routes.saturating_add(1);
        }
        let occupied = self
            .message_port_routes
            .len()
            .saturating_add(self.pending_message_port_messages.len())
            .saturating_add(new_routes)
            .saturating_add(additional_events);
        if occupied > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native MessagePort routes and queued events",
                MAX_NATIVE_WORKER_MESSAGES,
                occupied,
            ));
        }
        Ok(())
    }

    pub(super) fn register_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        self.validate_transfer_registration(transfers, 0)?;
        for transfer in transfers {
            self.message_port_routes.insert(
                transfer.bridge_key.clone(),
                NativeMessagePortRoute { worker_id },
            );
        }
        Ok(())
    }

    pub(super) fn remove_transfer_routes(&mut self, transfers: &[NativeMessagePortTransfer]) {
        for transfer in transfers {
            self.message_port_routes.remove(&transfer.bridge_key);
            self.pending_page_message_port_commands
                .retain(|command| command.bridge_key != transfer.bridge_key);
        }
        self.pending_message_port_messages.retain(|message| {
            message.close || self.message_port_routes.contains_key(&message.bridge_key)
        });
    }

    pub(super) fn queue_page_message_port(
        &mut self,
        worker_id: u32,
        bridge_key: String,
        data: serde_json::Value,
        transfer_ports: Vec<NativeMessagePortTransfer>,
        object_urls: Vec<NativeObjectUrlTransfer>,
    ) -> Result<(), NativeEngineError> {
        let Some(route) = self.message_port_routes.get(&bridge_key).cloned() else {
            return Ok(());
        };
        if route.worker_id != worker_id {
            return Err(NativeEngineError::invalid(
                "native MessagePort route",
                "worker id does not own the MessagePort bridge",
            ));
        }
        validate_native_object_url_transfers(&object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &data,
                "transfer_ports": &transfer_ports,
                "object_urls": &object_urls,
            }),
            "native page MessagePort event",
        )?;
        self.validate_transfer_registration(&transfer_ports, 1)?;
        self.register_worker_transfers(worker_id, &transfer_ports)?;
        self.pending_message_port_messages
            .push_back(NativeMessagePortPageMessage {
                bridge_key,
                data,
                close: false,
                transfer_ports,
                object_urls,
            });
        Ok(())
    }

    pub(super) fn retire_message_port_route(
        &mut self,
        bridge_key: &str,
    ) -> Option<NativeMessagePortRoute> {
        let route = self.message_port_routes.remove(bridge_key)?;
        self.pending_message_port_messages
            .retain(|message| message.bridge_key != bridge_key);
        self.pending_page_message_port_commands
            .retain(|command| command.bridge_key != bridge_key);
        Some(route)
    }

    pub(super) fn close_worker_message_port(
        &mut self,
        worker_id: u32,
        bridge_key: String,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("native MessagePort bridge key", &bridge_key)?;
        if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                bridge_key.len(),
            ));
        }
        let Some(route) = self.message_port_routes.get(&bridge_key) else {
            return Ok(());
        };
        if route.worker_id != worker_id {
            return Err(NativeEngineError::invalid(
                "native MessagePort route",
                "worker id does not own the MessagePort bridge",
            ));
        }
        if self.retire_message_port_route(&bridge_key).is_none() {
            return Ok(());
        }
        if self.pending_message_port_messages.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native page MessagePort messages",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_message_port_messages.len().saturating_add(1),
            ));
        }
        self.pending_message_port_messages
            .push_back(NativeMessagePortPageMessage {
                bridge_key,
                data: serde_json::Value::Null,
                close: true,
                transfer_ports: Vec::new(),
                object_urls: Vec::new(),
            });
        Ok(())
    }

    pub(super) fn remove_worker_routes(&mut self, worker_id: u32) {
        let stream_keys = self
            .worker_fetch_stream_connections
            .keys()
            .copied()
            .filter(|(stream_worker_id, _)| *stream_worker_id == worker_id)
            .collect::<Vec<_>>();
        for stream_key in stream_keys {
            if let Some(connection) = self.worker_fetch_stream_connections.remove(&stream_key) {
                let _ = connection
                    .commands
                    .try_send(NativeFetchStreamCommand::Cancel);
            }
        }
        let upload_keys = self
            .worker_fetch_upload_connections
            .keys()
            .copied()
            .filter(|(upload_worker_id, _)| *upload_worker_id == worker_id)
            .collect::<Vec<_>>();
        for upload_key in upload_keys {
            if let Some(connection) = self.worker_fetch_upload_connections.remove(&upload_key) {
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Cancel);
            }
        }
        let retired_bridge_keys = self
            .message_port_routes
            .iter()
            .filter_map(|(bridge_key, route)| {
                (route.worker_id == worker_id).then_some(bridge_key.clone())
            })
            .collect::<BTreeSet<_>>();
        self.message_port_routes
            .retain(|_, route| route.worker_id != worker_id);
        self.pending_page_message_port_commands
            .retain(|command| !retired_bridge_keys.contains(&command.bridge_key));
        self.shared_worker_keys
            .retain(|_, shared_worker_id| *shared_worker_id != worker_id);
        self.shared_worker_owners.remove(&worker_id);
        let live_bridge_keys = self
            .message_port_routes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.pending_message_port_messages
            .retain(|message| message.close || live_bridge_keys.contains(&message.bridge_key));
    }
}
