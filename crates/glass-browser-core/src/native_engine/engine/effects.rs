use super::*;

impl NativeEngine {
    pub(super) fn queue_download(
        &mut self,
        url: String,
        download_attribute: &str,
    ) -> Result<(), NativeEngineError> {
        if self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS {
            return Err(NativeEngineError::limit(
                "native pending downloads",
                MAX_NATIVE_PENDING_DOWNLOADS,
                self.pending_downloads.len().saturating_add(1),
            ));
        }
        let guid = format!("native-download-{}", self.next_download_id);
        self.next_download_id = self.next_download_id.saturating_add(1);
        let suggested_filename = native_download_filename(download_attribute, &url);
        self.pending_downloads.push_back(NativePendingDownload {
            guid,
            url,
            suggested_filename,
            target_id: self.config.context_id.clone(),
            frame_id: self.frame_id.clone(),
        });
        Ok(())
    }

    pub(super) fn queue_popup(&mut self, url: String) -> Result<(), NativeEngineError> {
        self.queue_popup_request(NativePopupRequest {
            url,
            target: "_blank".into(),
            method: NativeNavigationMethod::Get,
            body: None,
            body_content_type: None,
            handle: None,
            object_url: None,
            source_context_id: String::new(),
        })
    }

    pub(super) fn queue_popup_request(
        &mut self,
        mut popup: NativePopupRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_popups.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending popups",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_popups.len().saturating_add(1),
            ));
        }
        validate_url_text("popup target URL", &popup.url)?;
        validate_url_text("popup target name", &popup.target)?;
        validate_target_navigation_payload(
            popup.method,
            popup.body.as_ref(),
            popup.body_content_type.as_deref(),
            "popup navigation payload",
        )?;
        if let Some(handle) = popup.handle.as_deref() {
            validate_url_text("popup window handle", handle)?;
        }
        if popup.object_url.is_none()
            && popup
                .url
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
        {
            popup.object_url = self
                .javascript
                .as_ref()
                .map(|javascript| javascript.object_url_resource(&popup.url))
                .transpose()?
                .flatten();
        }
        if popup.source_context_id.is_empty() {
            popup.source_context_id = self.config.context_id.clone();
        } else {
            super::super::config::validate_context_id(&popup.source_context_id)?;
        }
        self.pending_popups.push_back(popup);
        Ok(())
    }

    pub(super) fn queue_popup_requests(
        &mut self,
        popups: Vec<NativePopupRequest>,
    ) -> Result<(), NativeEngineError> {
        for popup in popups {
            self.queue_popup_request(popup)?;
        }
        Ok(())
    }

    pub(super) fn queue_post_message_request(
        &mut self,
        mut message: NativePostMessageRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_post_messages.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending postMessage requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_post_messages.len().saturating_add(1),
            ));
        }
        if message.target.is_empty() && message.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "postMessage target",
                "must not be empty without a direct target context",
            ));
        }
        if !message.target.is_empty() {
            validate_url_text("postMessage target", &message.target)?;
        }
        validate_url_text("postMessage target origin", &message.target_origin)?;
        if let Some(target_context_id) = message.target_context_id.as_deref() {
            super::super::config::validate_context_id(target_context_id)?;
        }
        let encoded = serde_json::to_vec(&message.data).map_err(|_| NativeEngineError::Worker {
            operation: "queue native postMessage".into(),
            reason: "postMessage data could not be serialized".into(),
        })?;
        if encoded.len() > super::super::javascript::MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native postMessage data",
                super::super::javascript::MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_object_url_transfers(&message.object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native postMessage data",
        )?;
        if message.source_context_id.is_empty() {
            message.source_context_id = self.config.context_id.clone();
        } else {
            super::super::config::validate_context_id(&message.source_context_id)?;
        }
        if message.source_origin.is_empty() {
            message.source_origin = self.origin.serialized();
        } else {
            validate_url_text("postMessage source origin", &message.source_origin)?;
        }
        if message.source_frame_id.is_empty() {
            message.source_frame_id = self.frame_id.clone();
        } else {
            validate_context_id(&message.source_frame_id)?;
        }
        self.pending_post_messages.push_back(message);
        Ok(())
    }

    pub(super) fn queue_post_message_requests(
        &mut self,
        messages: Vec<NativePostMessageRequest>,
    ) -> Result<(), NativeEngineError> {
        for message in messages {
            self.queue_post_message_request(message)?;
        }
        Ok(())
    }

    pub(super) fn queue_post_message_requests_from_origin(
        &mut self,
        messages: Vec<NativePostMessageRequest>,
        origin: &NativeOrigin,
    ) -> Result<(), NativeEngineError> {
        for mut message in messages {
            if message.source_origin.is_empty() {
                message.source_origin = origin.serialized();
            }
            self.queue_post_message_request(message)?;
        }
        Ok(())
    }

    pub(crate) fn queue_page_message_port_commands(
        &mut self,
        commands: Vec<NativePageMessagePortCommand>,
    ) -> Result<(), NativeEngineError> {
        for mut command in commands {
            validate_page_message_port_command(&command)?;
            if self.pending_page_message_port_commands.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native pending page MessagePort commands",
                    MAX_NATIVE_EFFECTS,
                    self.pending_page_message_port_commands
                        .len()
                        .saturating_add(1),
                ));
            }
            if command.source_context_id.is_empty() {
                command.source_context_id = self.config.context_id.clone();
            } else {
                validate_context_id(&command.source_context_id)?;
            }
            if command.source_frame_id.is_empty() {
                command.source_frame_id = self.frame_id.clone();
            } else {
                validate_context_id(&command.source_frame_id)?;
            }
            self.pending_page_message_port_commands.push_back(command);
        }
        Ok(())
    }

    pub(super) fn queue_shared_worker_create_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
        constructor_origin: NativeOrigin,
        owner_url: String,
    ) -> Result<(), NativeEngineError> {
        if commands.len()
            > MAX_NATIVE_EFFECTS.saturating_sub(self.pending_shared_worker_creates.len())
        {
            return Err(NativeEngineError::limit(
                "native pending SharedWorker creates",
                MAX_NATIVE_EFFECTS,
                self.pending_shared_worker_creates
                    .len()
                    .saturating_add(commands.len()),
            ));
        }
        for command in commands {
            let NativeScriptCommand::SharedWorkerCreate {
                connection_id,
                href,
                name,
                worker_type,
                credentials,
                extended_lifetime,
                referrer_policy,
                constructor_storage_key: _,
                transfer_port,
            } = command
            else {
                return Err(NativeEngineError::Worker {
                    operation: "route SharedWorker create".into(),
                    reason: "content process returned an unexpected worker command".into(),
                });
            };
            if connection_id == 0 {
                return Err(NativeEngineError::invalid(
                    "native SharedWorker connection id",
                    "must be positive",
                ));
            }
            validate_context_id(&self.config.context_id)?;
            validate_context_id(&self.frame_id)?;
            validate_native_shared_worker_options(&worker_type, &credentials)?;
            referrer_policy
                .as_deref()
                .map(super::super::resource_loader::NativeFetchReferrerPolicy::parse)
                .transpose()?;
            validate_message_port_transfers(std::slice::from_ref(&transfer_port))?;
            let document_generation = self.document.generation();
            self.pending_shared_worker_creates
                .push_back(NativeSharedWorkerCreateRequest {
                    page_worker_id: connection_id,
                    source_context_id: self.config.context_id.clone(),
                    source_frame_id: self.frame_id.clone(),
                    document_generation,
                    constructor_origin: constructor_origin.clone(),
                    owner_url: owner_url.clone(),
                    href,
                    name,
                    worker_type,
                    credentials,
                    extended_lifetime,
                    referrer_policy,
                    transfer_port,
                });
        }
        Ok(())
    }

    pub(super) fn collect_page_message_port_commands(&mut self) -> Result<(), NativeEngineError> {
        let commands = self.workers.take_page_message_port_commands();
        self.queue_page_message_port_commands(commands)
    }

    pub(super) fn queue_window_close_request(
        &mut self,
        mut request: NativeWindowCloseRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_window_closes.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending window close requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_window_closes.len().saturating_add(1),
            ));
        }
        validate_url_text("window close target", &request.target)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            super::super::config::validate_context_id(target_context_id)?;
        }
        if request.source_context_id.is_empty() {
            request.source_context_id = self.config.context_id.clone();
        } else {
            super::super::config::validate_context_id(&request.source_context_id)?;
        }
        self.pending_window_closes.push_back(request);
        Ok(())
    }

    pub(super) fn queue_window_close_requests(
        &mut self,
        requests: Vec<NativeWindowCloseRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            self.queue_window_close_request(request)?;
        }
        Ok(())
    }

    pub(super) fn queue_window_navigation_request(
        &mut self,
        mut request: NativeWindowNavigationRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_window_navigations.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending window navigation requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_window_navigations.len().saturating_add(1),
            ));
        }
        if request.target.is_empty() && request.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "window navigation target",
                "must not be empty without a direct target context",
            ));
        }
        if !request.target.is_empty() {
            validate_url_text("window navigation target", &request.target)?;
        }
        validate_url_text("window navigation href", &request.href)?;
        validate_target_navigation_payload(
            request.method,
            request.body.as_ref(),
            request.body_content_type.as_deref(),
            "window navigation payload",
        )?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            super::super::config::validate_context_id(target_context_id)?;
        }
        if request.source_context_id.is_empty() {
            request.source_context_id = self.config.context_id.clone();
        } else {
            super::super::config::validate_context_id(&request.source_context_id)?;
        }
        self.pending_window_navigations.push_back(request);
        Ok(())
    }

    pub(super) fn queue_window_navigation_requests(
        &mut self,
        requests: Vec<NativeWindowNavigationRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            self.queue_window_navigation_request(request)?;
        }
        Ok(())
    }

    pub(super) fn queue_form_target_navigation(
        &mut self,
        target: &str,
        navigation: &NativeNavigationRequest,
    ) -> Result<bool, NativeEngineError> {
        validate_window_name(target)?;
        let target_kind = target.to_ascii_lowercase();
        let target_context = match target_kind.as_str() {
            "_parent" => self
                .frame_script_context
                .as_ref()
                .and_then(|context| context.parent.as_ref()),
            "_top" | "_unfencedtop" => self
                .frame_script_context
                .as_ref()
                .and_then(|context| context.top.as_ref()),
            "_self" => None,
            _ => {
                self.queue_popup_request(NativePopupRequest {
                    url: navigation.url.clone(),
                    target: target.to_owned(),
                    method: navigation.method,
                    body: navigation.body.clone(),
                    body_content_type: navigation.body_content_type.clone(),
                    handle: None,
                    object_url: navigation.object_url.clone(),
                    source_context_id: String::new(),
                })?;
                return Ok(true);
            }
        };
        let Some(target_context) = target_context else {
            return Ok(false);
        };
        if target_context.context_id == self.config.context_id {
            return Ok(false);
        }
        self.queue_window_navigation_request(NativeWindowNavigationRequest {
            target: String::new(),
            target_context_id: Some(target_context.context_id.clone()),
            href: navigation.url.clone(),
            method: navigation.method,
            body: navigation.body.clone(),
            body_content_type: navigation.body_content_type.clone(),
            replace: navigation.replace_history,
            object_url: navigation.object_url.clone(),
            source_context_id: String::new(),
        })?;
        Ok(true)
    }

    pub(super) fn navigate_local_form_request(
        &mut self,
        mut request: NativeNavigationRequest,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let target = request.target.take().unwrap_or_else(|| "_self".into());
        let target_kind = target.to_ascii_lowercase();
        let targets_current_document = match target_kind.as_str() {
            "_self" => true,
            "_parent" | "_top" | "_unfencedtop" => self.frame_script_context.is_none(),
            _ => false,
        };
        if !targets_current_document {
            if !self.allows_top_level_navigation(&request.url)? {
                return Ok(NativeActionResult {
                    revision: self.revision,
                    accepted: true,
                });
            }
            if self.queue_form_target_navigation(&target, &request)? {
                return Ok(NativeActionResult {
                    revision: self.revision,
                    accepted: true,
                });
            }
        }
        let snapshot = self.navigate(request.url)?;
        Ok(NativeActionResult {
            revision: snapshot.revision,
            accepted: true,
        })
    }

    pub(super) fn queue_service_worker_open_window_request(
        &mut self,
        mut request: NativeServiceWorkerOpenWindowRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_service_worker_open_windows.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending service worker openWindow requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_service_worker_open_windows
                    .len()
                    .saturating_add(1),
            ));
        }
        if request.request_id == 0 || request.worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "service worker openWindow request",
                "request and worker ids must be positive",
            ));
        }
        validate_url_text("service worker openWindow URL", &request.url)?;
        if request.source_context_id.is_empty() {
            request.source_context_id = self.config.context_id.clone();
        } else {
            super::super::config::validate_context_id(&request.source_context_id)?;
        }
        if request.source_frame_id.is_empty() {
            request.source_frame_id = self.frame_id.clone();
        } else {
            super::super::config::validate_context_id(&request.source_frame_id)?;
        }
        self.pending_service_worker_open_windows.push_back(request);
        Ok(())
    }

    pub(super) fn queue_service_worker_open_window_requests(
        &mut self,
        requests: Vec<NativeServiceWorkerOpenWindowRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            self.queue_service_worker_open_window_request(request)?;
        }
        Ok(())
    }

    pub(super) fn queue_service_worker_client_messages(
        &mut self,
        messages: Vec<NativeServiceWorkerClientMessage>,
    ) -> Result<(), NativeEngineError> {
        if self
            .pending_service_worker_client_messages
            .len()
            .saturating_add(messages.len())
            > MAX_NATIVE_PENDING_POPUPS
        {
            return Err(NativeEngineError::limit(
                "native pending service worker client messages",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_service_worker_client_messages
                    .len()
                    .saturating_add(messages.len()),
            ));
        }
        for message in messages {
            if message.worker_id == 0 {
                return Err(NativeEngineError::invalid(
                    "service worker client message worker id",
                    "must be positive",
                ));
            }
            if message.client_id.is_empty() {
                return Err(NativeEngineError::invalid(
                    "service worker client message id",
                    "must not be empty",
                ));
            }
            self.pending_service_worker_client_messages
                .push_back(message);
        }
        Ok(())
    }

    pub(super) fn queue_frame_script_requests(
        &mut self,
        requests: Vec<NativeFrameScriptRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            if self.pending_frame_scripts.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native pending frame script requests",
                    MAX_NATIVE_EFFECTS,
                    self.pending_frame_scripts.len().saturating_add(1),
                ));
            }
            validate_context_id(&request.frame_id)?;
            validate_context_id(&request.source_frame_id)?;
            if matches!(
                request.command.as_ref(),
                NativeScriptCommand::FrameScript { .. }
            ) {
                return Err(NativeEngineError::invalid(
                    "native frame script command",
                    "nested frame script commands are not allowed",
                ));
            }
            self.pending_frame_scripts.push_back(request);
        }
        Ok(())
    }

    pub fn take_pending_popups(&mut self) -> Vec<NativePopupRequest> {
        self.pending_popups.drain(..).collect()
    }

    pub fn take_pending_content_cookie_changes(
        &self,
    ) -> Result<Vec<NativeCookieChange>, NativeEngineError> {
        let mut changes =
            self.pending_content_cookie_changes
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "take content process cookie changes".into(),
                    reason: "content process cookie change queue is poisoned".into(),
                })?;
        Ok(std::mem::take(&mut *changes))
    }

    pub fn take_pending_post_messages(&mut self) -> Vec<NativePostMessageRequest> {
        self.pending_post_messages.drain(..).collect()
    }

    pub fn take_pending_page_message_port_commands(&mut self) -> Vec<NativePageMessagePortCommand> {
        self.pending_page_message_port_commands.drain(..).collect()
    }

    pub fn take_pending_shared_worker_creates(&mut self) -> Vec<NativeSharedWorkerCreateRequest> {
        let committed_generation = self.document.generation();
        self.pending_shared_worker_creates
            .drain(..)
            .map(|mut request| {
                // HTTP load effects are queued before their Document commits.
                // Bind the owner at extraction, after the commit establishes
                // the generation that this request belongs to.
                request.document_generation = committed_generation;
                request
            })
            .collect()
    }

    pub fn take_pending_window_closes(&mut self) -> Vec<NativeWindowCloseRequest> {
        self.pending_window_closes.drain(..).collect()
    }

    pub fn take_pending_window_navigations(&mut self) -> Vec<NativeWindowNavigationRequest> {
        self.pending_window_navigations.drain(..).collect()
    }

    pub fn take_pending_service_worker_open_windows(
        &mut self,
    ) -> Vec<NativeServiceWorkerOpenWindowRequest> {
        self.pending_service_worker_open_windows.drain(..).collect()
    }

    pub fn take_pending_service_worker_client_messages(
        &mut self,
    ) -> Vec<NativeServiceWorkerClientMessage> {
        self.pending_service_worker_client_messages
            .drain(..)
            .collect()
    }

    pub fn take_pending_frame_scripts(&mut self) -> Vec<NativeFrameScriptRequest> {
        self.pending_frame_scripts.drain(..).collect()
    }

    /// Apply one already-authorized command emitted by a same-origin parent
    /// realm to this frame's own document owner. The command enters through
    /// the normal JavaScript host path so event listeners, navigation, and
    /// child effects retain their ordinary ordering.
    pub(crate) async fn apply_frame_script_command_async(
        &mut self,
        command: NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        validate_frame_script_command(&command)?;
        let mut page_events = NativePageEventBatch::default();
        page_events.frame_script_commands.push(command);
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub async fn apply_frame_script_command_with_effects_async(
        &mut self,
        command: NativeScriptCommand,
    ) -> Result<Vec<NativeEffect>, NativeEngineError> {
        let previous_revision = self.revision();
        self.apply_frame_script_command_async(command).await?;
        Ok(self.effects_since(previous_revision)?.effects)
    }

    /// Deliver events observed by a child engine to the parent realm's
    /// same-origin projection. The backend supplies only typed effect records;
    /// the parent runtime resolves node identity from its current binding.
    pub async fn dispatch_frame_events_async(
        &mut self,
        frame_id: &str,
        effects: &[NativeEffect],
    ) -> Result<(), NativeEngineError> {
        let metadata = effects
            .iter()
            .map(|effect| {
                (
                    effect.node_id.index(),
                    effect.node_id.generation(),
                    effect.kind,
                )
            })
            .collect::<Vec<_>>();
        let Some(batch) = frame_event_batch(frame_id, &metadata)? else {
            return Ok(());
        };
        let mut page_events = NativePageEventBatch::default();
        page_events.frame_event_batches.push(batch);
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub async fn dispatch_post_message(
        &mut self,
        source_context_id: &str,
        source_origin: &str,
        data: &serde_json::Value,
        transfer_ports: &[NativeMessagePortTransfer],
        object_urls: &[super::super::resource_loader::NativeObjectUrlTransfer],
    ) -> Result<(), NativeEngineError> {
        self.require_running("message event")?;
        validate_message_port_transfers(transfer_ports)?;
        validate_native_object_url_transfers(object_urls)?;
        let mut page_events = NativePageEventBatch::default();
        page_events
            .post_message_events
            .push(NativePageMessageEvent {
                source_context_id: source_context_id.to_owned(),
                source_origin: source_origin.to_owned(),
                data: data.clone(),
                transfer_ports: transfer_ports.to_vec(),
                object_urls: object_urls.to_vec(),
            });
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub async fn dispatch_worker_messages_async(
        &mut self,
        messages: &[NativeWorkerMessage],
    ) -> Result<(), NativeEngineError> {
        self.require_running("Worker error event")?;
        if messages.is_empty() {
            return Ok(());
        }
        if messages.len() > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                "native Worker event batch",
                MAX_NATIVE_WORKER_MESSAGES,
                messages.len(),
            ));
        }
        let mut page_events = NativePageEventBatch::default();
        page_events.worker_messages.extend_from_slice(messages);
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub async fn dispatch_page_message_port(
        &mut self,
        bridge_key: &str,
        data: &serde_json::Value,
        transfer_ports: &[NativeMessagePortTransfer],
        object_urls: &[NativeObjectUrlTransfer],
    ) -> Result<(), NativeEngineError> {
        self.require_running("MessagePort event")?;
        let command = NativePageMessagePortCommand {
            bridge_key: bridge_key.to_owned(),
            data: data.clone(),
            close: false,
            transfer_ports: transfer_ports.to_vec(),
            object_urls: object_urls.to_vec(),
            source_context_id: String::new(),
            source_frame_id: String::new(),
        };
        validate_page_message_port_command(&command)?;
        let mut page_events = NativePageEventBatch::default();
        page_events
            .message_port_messages
            .push(NativeMessagePortPageMessage {
                bridge_key: command.bridge_key,
                data: command.data,
                close: false,
                transfer_ports: command.transfer_ports,
                object_urls: command.object_urls,
            });
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub async fn dispatch_page_message_port_close(
        &mut self,
        bridge_key: &str,
    ) -> Result<(), NativeEngineError> {
        self.require_running("MessagePort close event")?;
        validate_url_text("native MessagePort bridge key", bridge_key)?;
        if bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                bridge_key.len(),
            ));
        }
        let mut page_events = NativePageEventBatch::default();
        page_events
            .message_port_messages
            .push(NativeMessagePortPageMessage {
                bridge_key: bridge_key.to_owned(),
                data: serde_json::Value::Null,
                close: true,
                transfer_ports: Vec::new(),
                object_urls: Vec::new(),
            });
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }
}
