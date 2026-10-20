use super::*;

impl NativeServiceWorkerRegistry {
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

    pub(super) fn register_transfers(
        &mut self,
        worker_id: u32,
        transfers: &[NativeMessagePortTransfer],
    ) -> Result<(), NativeEngineError> {
        self.validate_transfer_registration(transfers, 0)?;
        for transfer in transfers {
            self.message_port_routes
                .insert(transfer.bridge_key.clone(), worker_id);
        }
        Ok(())
    }

    pub(super) fn validate_transfer_registration(
        &self,
        transfers: &[NativeMessagePortTransfer],
        additional_events: usize,
    ) -> Result<(), NativeEngineError> {
        if transfers.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Service Worker MessagePort transfers",
                MAX_NATIVE_WORKER_MESSAGES,
                transfers.len(),
            ));
        }
        validate_message_port_transfers(transfers)?;
        let mut new_routes = 0usize;
        for transfer in transfers {
            if self.message_port_routes.contains_key(&transfer.bridge_key) {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort transfer",
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
                "native Service Worker MessagePort routes and queued events",
                MAX_NATIVE_WORKER_MESSAGES,
                occupied,
            ));
        }
        Ok(())
    }

    pub(super) fn remove_transfer_routes(&mut self, transfers: &[NativeMessagePortTransfer]) {
        for transfer in transfers {
            self.message_port_routes.remove(&transfer.bridge_key);
        }
        let live_bridge_keys = self
            .message_port_routes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.pending_message_port_messages
            .retain(|message| message.close || live_bridge_keys.contains(&message.bridge_key));
    }

    pub(super) fn retire_message_port_route(&mut self, bridge_key: &str) -> Option<u32> {
        let worker_id = self.message_port_routes.remove(bridge_key)?;
        self.pending_message_port_messages
            .retain(|message| message.bridge_key != bridge_key);
        Some(worker_id)
    }

    pub(super) fn close_worker_message_port(
        &mut self,
        worker_id: u32,
        bridge_key: &str,
    ) -> Result<(), NativeEngineError> {
        validate_service_worker_message_port_bridge_key(bridge_key)?;
        let Some(route_worker_id) = self.message_port_routes.get(bridge_key).copied() else {
            return Ok(());
        };
        if route_worker_id != worker_id {
            return Err(NativeEngineError::invalid(
                "native service-worker MessagePort route",
                "worker id does not own the MessagePort bridge",
            ));
        }
        if self.pending_message_port_messages.len() >= MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Service Worker MessagePort close events",
                MAX_NATIVE_WORKER_MESSAGES,
                self.pending_message_port_messages.len().saturating_add(1),
            ));
        }
        if self.retire_message_port_route(bridge_key).is_none() {
            return Ok(());
        }
        self.pending_message_port_messages
            .push_back(NativeMessagePortPageMessage {
                bridge_key: bridge_key.to_owned(),
                data: Value::Null,
                close: true,
                transfer_ports: Vec::new(),
                object_urls: Vec::new(),
            });
        Ok(())
    }

    pub(super) fn collect_message_port_commands(
        &mut self,
        worker_id: u32,
        commands: Vec<NativeScriptCommand>,
    ) -> Result<(), NativeEngineError> {
        if commands.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native service-worker MessagePort commands",
                MAX_NATIVE_WORKER_MESSAGES,
                commands.len(),
            ));
        }
        for command in commands {
            if let NativeScriptCommand::MessagePortClose {
                bridge_key,
                worker_id: Some(command_worker_id),
            } = &command
            {
                if *command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native service-worker MessagePort command",
                        "service worker command owner is invalid",
                    ));
                }
                self.close_worker_message_port(worker_id, bridge_key)?;
                continue;
            }
            let NativeScriptCommand::MessagePortPostMessage {
                bridge_key,
                data,
                worker_id: Some(command_worker_id),
                transfer_ports,
                object_urls,
            } = command
            else {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort command",
                    "service worker emitted an invalid MessagePort host command",
                ));
            };
            if command_worker_id != worker_id {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort command",
                    "service worker command owner is invalid",
                ));
            }
            let Some(route_worker_id) = self.message_port_routes.get(&bridge_key).copied() else {
                continue;
            };
            if route_worker_id != worker_id {
                return Err(NativeEngineError::invalid(
                    "native service-worker MessagePort route",
                    "worker id does not own the MessagePort bridge",
                ));
            }
            validate_service_worker_message_port_bridge_key(&bridge_key)?;
            validate_native_object_url_transfers(&object_urls)?;
            validate_native_message_payload(
                &serde_json::json!({
                    "data": &data,
                    "transfer_ports": &transfer_ports,
                    "object_urls": &object_urls,
                }),
                "native service-worker MessagePort event",
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
        }
        Ok(())
    }

    pub(super) fn remove_worker_routes(&mut self, worker_id: u32) {
        self.message_port_routes
            .retain(|_, route_worker_id| *route_worker_id != worker_id);
        for ((pending_worker_id, _), abort_handle) in &self.lifetime_fetch_abort_handles {
            if *pending_worker_id == worker_id {
                abort_handle.abort();
            }
        }
        let live_bridge_keys = self
            .message_port_routes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.pending_message_port_messages
            .retain(|message| message.close || live_bridge_keys.contains(&message.bridge_key));
        self.pending_open_windows
            .retain(|request| request.worker_id != worker_id);
        self.pending_fetches
            .retain(|(pending_worker_id, _), _| *pending_worker_id != worker_id);
        self.pending_parent_lifetime_fetches
            .retain(|task| task.worker_id != worker_id);
        self.completed_fetches
            .retain(|completion| completion.worker_id != worker_id);
        self.announced_open_windows
            .retain(|(request_worker_id, _)| *request_worker_id != worker_id);
    }
}
