use super::*;

impl NativeContentFetchBroker<'_> {
    pub(super) async fn receive_parent_response(
        &mut self,
        operation: &'static str,
    ) -> Result<Vec<u8>, NativeEngineError> {
        receive_content_ipc_response(self.ipc_requests, self.cancelled_parent_fetches, operation)
            .await
    }

    pub(crate) async fn cancel_parent_fetch(
        &mut self,
        fetch_id: u32,
    ) -> Result<(), NativeEngineError> {
        if fetch_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent Fetch cancellation ID",
                "must be a positive integer",
            ));
        }
        let key = (self.request_id, fetch_id);
        if self.cancelled_parent_fetches.len() >= MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "cancelled parent Fetch requests",
                MAX_NATIVE_EFFECTS,
                self.cancelled_parent_fetches.len().saturating_add(1),
            ));
        }
        if !self.cancelled_parent_fetches.insert(key) {
            return Err(NativeEngineError::invalid(
                "parent Fetch cancellation",
                "request was already cancelled",
            ));
        }
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_cancel",
                "id": self.request_id,
                "fetch_id": fetch_id,
            }),
        )
        .await
    }

    pub(super) fn take_cookie_writes(
        &self,
    ) -> Result<Vec<NativeContentCookieWrite>, NativeEngineError> {
        let cookie_updates = self
            .runtime
            .map(NativeJavaScriptRuntime::take_cookie_updates)
            .unwrap_or_default();
        if cookie_updates.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "parent-owned document.cookie writes",
                MAX_NATIVE_EFFECTS,
                cookie_updates.len(),
            ));
        }
        Ok(cookie_updates
            .into_iter()
            .map(|value| NativeContentCookieWrite {
                owner: self.owner.clone(),
                value,
            })
            .collect())
    }

    pub(super) fn update_runtime_cookie_projection(&self, document_cookie: &str) {
        if let Some(runtime) = self.runtime {
            runtime.set_cookie_state(document_cookie.to_owned());
        }
    }

    pub(crate) async fn fetch(
        &mut self,
        fetch_id: u32,
        request: &NativeFetchRequest<'_>,
    ) -> Result<FetchBrokerResult, NativeEngineError> {
        let captured_load_fetch = self.captured_load_fetches;
        self.fetch_inner(fetch_id, request, captured_load_fetch, false)
            .await
    }

    pub(super) async fn open_event_source(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        initiator_url: &str,
        href: &str,
        with_credentials: bool,
        last_event_id: &str,
    ) -> Result<NativeParentEventSourceOpenResult, NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) {
            return Err(NativeEngineError::invalid(
                "parent EventSource source ID",
                "source and optional worker IDs must be positive",
            ));
        }
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_event_source_open_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "owner": self.owner,
                "document_url": self.owner.document_url,
                "initiator_url": initiator_url,
                "href": href,
                "with_credentials": with_credentials,
                "last_event_id": last_event_id,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent EventSource open")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent EventSource open response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid EventSource open JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent EventSource open response",
                NativeWorkerFailureKind::Protocol,
                "EventSource open response belongs to a different request",
            ));
        }
        self.update_parent_document_cookie(&response, "parent EventSource open")?;
        let csp_violations = response
            .get("csp_violations")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent EventSource open response",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned invalid EventSource CSP violations",
                )
            })?
            .unwrap_or_default();
        if response.get("kind").and_then(Value::as_str) == Some("parent_event_source_error") {
            return Ok(NativeParentEventSourceOpenResult::Failed {
                message: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the EventSource request")
                    .to_owned(),
                csp_violations,
            });
        }
        if response.get("kind").and_then(Value::as_str) != Some("parent_event_source_opened") {
            return Err(NativeEngineError::worker_failure(
                "decode parent EventSource open response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown EventSource open result",
            ));
        }
        let stream_id = response
            .get("stream_id")
            .and_then(Value::as_u64)
            .filter(|stream_id| *stream_id > 0)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent EventSource open response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted a valid EventSource stream ID",
                )
            })?;
        let origin = response
            .get("origin")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent EventSource open response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the EventSource origin",
                )
            })?
            .to_owned();
        Ok(NativeParentEventSourceOpenResult::Opened(
            NativeParentEventSourceOpen {
                stream_id,
                origin,
                csp_violations,
            },
        ))
    }

    pub(super) async fn read_parent_event_source(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        stream_id: u64,
    ) -> Result<NativeParentEventSourceRead, NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) || stream_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent EventSource stream identity",
                "source, optional worker, and stream IDs must be positive",
            ));
        }
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_event_source_read_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "stream_id": stream_id,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent EventSource read")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent EventSource chunk",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid EventSource chunk JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
            || response.get("stream_id").and_then(Value::as_u64) != Some(stream_id)
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent EventSource chunk",
                NativeWorkerFailureKind::Protocol,
                "EventSource chunk belongs to a different request or stream",
            ));
        }
        self.update_parent_document_cookie(&response, "parent EventSource read")?;
        match response.get("kind").and_then(Value::as_str) {
            Some("parent_event_source_chunk") => {
                let encoded = response
                    .get("chunk_base64")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent EventSource chunk",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted EventSource chunk bytes",
                        )
                    })?;
                let chunk = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| {
                        NativeEngineError::worker_failure(
                            "decode parent EventSource chunk",
                            NativeWorkerFailureKind::Protocol,
                            "parent EventSource chunk is not valid base64",
                        )
                    })?;
                let maximum_chunk =
                    MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES + MAX_NATIVE_EVENTSOURCE_FIELD_BYTES;
                if chunk.len() > maximum_chunk {
                    return Err(NativeEngineError::limit(
                        "parent EventSource chunk",
                        maximum_chunk,
                        chunk.len(),
                    ));
                }
                Ok(NativeParentEventSourceRead::Chunk(chunk))
            }
            Some("parent_event_source_pending") => Ok(NativeParentEventSourceRead::Pending),
            Some("parent_event_source_end") => Ok(NativeParentEventSourceRead::End),
            Some("parent_event_source_error") => Err(NativeEngineError::Network {
                operation: "EventSource stream".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent EventSource stream failed")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent EventSource chunk",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown EventSource chunk result",
            )),
        }
    }

    pub(super) async fn close_parent_event_source(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        stream_id: u64,
    ) -> Result<(), NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) || stream_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent EventSource stream identity",
                "source, optional worker, and stream IDs must be positive",
            ));
        }
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_event_source_close_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "stream_id": stream_id,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent EventSource close")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent EventSource close response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid EventSource close JSON",
            )
        })?;
        if !matches!(
            response.get("kind").and_then(Value::as_str),
            Some("parent_event_source_closed") | Some("parent_event_source_end")
        ) || response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
            || response.get("stream_id").and_then(Value::as_u64) != Some(stream_id)
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent EventSource close response",
                NativeWorkerFailureKind::Protocol,
                "EventSource close response belongs to a different request or stream",
            ));
        }
        self.update_parent_document_cookie(&response, "parent EventSource close")?;
        Ok(())
    }

    pub(super) async fn open_websocket(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        initiator_url: &str,
        href: &str,
        protocols: &[String],
    ) -> Result<NativeParentWebSocketOpenResult, NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) {
            return Err(NativeEngineError::invalid(
                "parent WebSocket source ID",
                "source and optional worker IDs must be positive",
            ));
        }
        validate_websocket_protocols(protocols)?;
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_websocket_open_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "owner": self.owner,
                "document_url": self.owner.document_url,
                "initiator_url": initiator_url,
                "href": href,
                "protocols": protocols,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent WebSocket open")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent WebSocket open response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid WebSocket open JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
            || decode_optional_content_worker_id(&response, "parent WebSocket worker ID")?
                != worker_id
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent WebSocket open response",
                NativeWorkerFailureKind::Protocol,
                "WebSocket open response belongs to a different request or worker",
            ));
        }
        self.update_parent_document_cookie(&response, "parent WebSocket open")?;
        let csp_violations = response
            .get("csp_violations")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent WebSocket open response",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned invalid WebSocket CSP violations",
                )
            })?
            .unwrap_or_default();
        if response.get("kind").and_then(Value::as_str) == Some("parent_websocket_error") {
            return Ok(NativeParentWebSocketOpenResult::Failed {
                message: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the WebSocket request")
                    .to_owned(),
                csp_violations,
            });
        }
        if response.get("kind").and_then(Value::as_str) != Some("parent_websocket_opened") {
            return Err(NativeEngineError::worker_failure(
                "decode parent WebSocket open response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown WebSocket open result",
            ));
        }
        let stream_id = response
            .get("stream_id")
            .and_then(Value::as_u64)
            .filter(|stream_id| *stream_id > 0)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent WebSocket open response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted a valid WebSocket stream ID",
                )
            })?;
        let protocol = response
            .get("protocol")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent WebSocket open response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the selected WebSocket protocol",
                )
            })?
            .to_owned();
        Ok(NativeParentWebSocketOpenResult::Opened(
            NativeParentWebSocketOpen {
                stream_id,
                protocol,
                csp_violations,
            },
        ))
    }

    pub(super) async fn read_parent_websocket(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        stream_id: u64,
    ) -> Result<NativeParentWebSocketRead, NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) || stream_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent WebSocket stream identity",
                "source, optional worker, and stream IDs must be positive",
            ));
        }
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_websocket_read_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "stream_id": stream_id,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent WebSocket read")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent WebSocket event",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid WebSocket event JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
            || response.get("worker_id").and_then(Value::as_u64) != worker_id.map(u64::from)
            || response.get("stream_id").and_then(Value::as_u64) != Some(stream_id)
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent WebSocket event",
                NativeWorkerFailureKind::Protocol,
                "WebSocket event belongs to a different request, worker, or stream",
            ));
        }
        self.update_parent_document_cookie(&response, "parent WebSocket read")?;
        let csp_violations = response
            .get("csp_violations")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent WebSocket event",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned invalid WebSocket CSP violations",
                )
            })?
            .unwrap_or_default();
        let origin = response
            .get("origin")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let event = match response.get("kind").and_then(Value::as_str) {
            Some("parent_websocket_pending") => return Ok(NativeParentWebSocketRead::Pending),
            Some("parent_websocket_text") => {
                let data = response
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent WebSocket text",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted WebSocket text data",
                        )
                    })?
                    .to_owned();
                if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                    return Err(NativeEngineError::limit(
                        "parent WebSocket text message",
                        MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                        data.len(),
                    ));
                }
                NativeWebSocketEvent::MessageText { data, origin }
            }
            Some("parent_websocket_binary") => {
                let encoded = response
                    .get("data_base64")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent WebSocket binary",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted WebSocket binary data",
                        )
                    })?;
                let data = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| {
                        NativeEngineError::worker_failure(
                            "decode parent WebSocket binary",
                            NativeWorkerFailureKind::Protocol,
                            "parent WebSocket binary data is not valid base64",
                        )
                    })?;
                if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                    return Err(NativeEngineError::limit(
                        "parent WebSocket binary message",
                        MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                        data.len(),
                    ));
                }
                NativeWebSocketEvent::MessageBinary { data, origin }
            }
            Some("parent_websocket_close") => {
                let code = response
                    .get("code")
                    .and_then(Value::as_u64)
                    .and_then(|code| u16::try_from(code).ok())
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent WebSocket close",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted a valid WebSocket close code",
                        )
                    })?;
                let reason = response
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent WebSocket close",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted the WebSocket close reason",
                        )
                    })?
                    .to_owned();
                NativeWebSocketEvent::Close {
                    code,
                    reason,
                    was_clean: response
                        .get("was_clean")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                }
            }
            Some("parent_websocket_error") => NativeWebSocketEvent::Error {
                message: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent WebSocket stream failed")
                    .to_owned(),
                csp_violations,
            },
            _ => {
                return Err(NativeEngineError::worker_failure(
                    "decode parent WebSocket event",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned an unknown WebSocket event",
                ));
            }
        };
        Ok(NativeParentWebSocketRead::Event(event))
    }

    pub(super) async fn send_parent_websocket(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        stream_id: u64,
        message: Message,
    ) -> Result<(), NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) || stream_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent WebSocket stream identity",
                "source, optional worker, and stream IDs must be positive",
            ));
        }
        let message = match message {
            Message::Text(data) if data.len() <= MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES => {
                json!({"message_type":"text","data":data.as_str()})
            }
            Message::Binary(data) if data.len() <= MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES => {
                json!({
                    "message_type":"binary",
                    "data_base64":base64::engine::general_purpose::STANDARD.encode(data),
                })
            }
            Message::Text(data) => {
                return Err(NativeEngineError::limit(
                    "parent WebSocket text message",
                    MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                    data.len(),
                ));
            }
            Message::Binary(data) => {
                return Err(NativeEngineError::limit(
                    "parent WebSocket binary message",
                    MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                    data.len(),
                ));
            }
            _ => {
                return Err(NativeEngineError::invalid(
                    "parent WebSocket message",
                    "only text and binary data messages are supported",
                ));
            }
        };
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_websocket_send_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "stream_id": stream_id,
                "owner": self.owner,
                "message": message,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let response = self
            .receive_parent_response("parent WebSocket send")
            .await?;
        let response: Value = serde_json::from_slice(&response).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent WebSocket send response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid WebSocket send JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
            || response.get("worker_id").and_then(Value::as_u64) != worker_id.map(u64::from)
            || response.get("stream_id").and_then(Value::as_u64) != Some(stream_id)
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent WebSocket send response",
                NativeWorkerFailureKind::Protocol,
                "WebSocket send acknowledgement does not match its stream",
            ));
        }
        self.update_parent_document_cookie(&response, "parent WebSocket send")?;
        match response.get("kind").and_then(Value::as_str) {
            Some("parent_websocket_sent") => Ok(()),
            Some("parent_websocket_error") => Err(NativeEngineError::Network {
                operation: "WebSocket send".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent WebSocket stream failed")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent WebSocket send response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown WebSocket send result",
            )),
        }
    }

    pub(super) async fn close_parent_websocket(
        &mut self,
        source_id: u32,
        worker_id: Option<u32>,
        stream_id: u64,
        code: u16,
        reason: &str,
    ) -> Result<bool, NativeEngineError> {
        if source_id == 0 || worker_id == Some(0) || stream_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent WebSocket stream identity",
                "source, optional worker, and stream IDs must be positive",
            ));
        }
        if code != 1000 && !(3000..=4999).contains(&code) {
            return Err(NativeEngineError::invalid(
                "parent WebSocket close code",
                "must be 1000 or in the 3000-4999 range",
            ));
        }
        if reason.len() > MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES {
            return Err(NativeEngineError::limit(
                "parent WebSocket close reason",
                MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
                reason.len(),
            ));
        }
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_websocket_close_request",
                "id": self.request_id,
                "source_id": source_id,
                "worker_id": worker_id,
                "stream_id": stream_id,
                "code": code,
                "reason": reason,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent WebSocket close")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent WebSocket close response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid WebSocket close JSON",
            )
        })?;
        if response.get("kind").and_then(Value::as_str) != Some("parent_websocket_closed")
            || response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("source_id").and_then(Value::as_u64) != Some(u64::from(source_id))
            || response.get("worker_id").and_then(Value::as_u64) != worker_id.map(u64::from)
            || response.get("stream_id").and_then(Value::as_u64) != Some(stream_id)
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent WebSocket close response",
                NativeWorkerFailureKind::Protocol,
                "WebSocket close acknowledgement does not match its stream",
            ));
        }
        self.update_parent_document_cookie(&response, "parent WebSocket close")?;
        Ok(response
            .get("was_clean")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    pub(super) fn update_parent_document_cookie(
        &mut self,
        response: &Value,
        operation: &'static str,
    ) -> Result<(), NativeEngineError> {
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    operation,
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        let document_cookie = document_cookie.to_owned();
        *self.document_cookie_projection = Some(document_cookie.clone());
        self.update_runtime_cookie_projection(&document_cookie);
        Ok(())
    }

    pub(crate) async fn fetch_for_captured_load(
        &mut self,
        fetch_id: u32,
        request: &NativeFetchRequest<'_>,
        top_level_navigation: bool,
    ) -> Result<FetchBrokerResult, NativeEngineError> {
        self.fetch_inner(fetch_id, request, true, top_level_navigation)
            .await
    }

    pub(crate) async fn fetch_for_captured_navigation(
        &mut self,
        fetch_id: u32,
        request: &NativeFetchRequest<'_>,
    ) -> Result<FetchBrokerResult, NativeEngineError> {
        self.fetch_inner(fetch_id, request, true, true).await
    }

    pub(super) async fn fetch_inner(
        &mut self,
        fetch_id: u32,
        request: &NativeFetchRequest<'_>,
        captured_load_fetch: bool,
        top_level_navigation: bool,
    ) -> Result<FetchBrokerResult, NativeEngineError> {
        let cookie_writes = self.take_cookie_writes()?;
        let page_meta_csp = (without_fragment(request.document_url)
            == without_fragment(&self.owner.document_url))
        .then_some(&self.page_meta_content_security_policies);
        let body = request
            .body
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| NativeEngineError::Worker {
                operation: "encode parent Fetch request".into(),
                reason: "request body could not be encoded".into(),
            })?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": fetch_id,
                "document_url": request.document_url,
                "captured_load_fetch": captured_load_fetch,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "page_meta_csp": page_meta_csp,
                "href": request.href,
                "credentials": request.credentials,
                "fetch_request": {
                    "method": request.method.as_str(),
                    "body": body,
                    "content_type": request.content_type,
                    "request_headers": request.request_headers,
                    "cors_mode": match request.cors_mode {
                        NativeCorsMode::Cors => "cors",
                        NativeCorsMode::NoCors => "no-cors",
                        NativeCorsMode::SameOrigin => "same-origin",
                        NativeCorsMode::Navigation => "navigation",
                    },
                    "redirect_mode": match request.redirect_mode {
                        NativeFetchRedirectMode::Follow => "follow",
                        NativeFetchRedirectMode::Error => "error",
                        NativeFetchRedirectMode::Manual => "manual",
                    },
                    "cache_mode": match request.cache_mode {
                        NativeFetchCacheMode::Default => "default",
                        NativeFetchCacheMode::NoStore => "no-store",
                        NativeFetchCacheMode::Reload => "reload",
                        NativeFetchCacheMode::NoCache => "no-cache",
                        NativeFetchCacheMode::ForceCache => "force-cache",
                        NativeFetchCacheMode::OnlyIfCached => "only-if-cached",
                    },
                    "timeout_ms": request.timeout.map(|timeout| timeout.as_millis()),
                    "max_response_bytes": request.max_response_bytes,
                    "top_level_navigation": top_level_navigation,
                    "credentials_mode": request.credentials_mode.map(|mode| mode.as_str()),
                    "referrer_url": request.referrer_url,
                    "referrer_policy": request.referrer_policy,
                },
            }),
        )
        .await?;
        let payload = self.receive_parent_response("parent Fetch broker").await?;
        let response: Value =
            serde_json::from_slice(&payload).map_err(|_| NativeEngineError::Worker {
                operation: "decode parent Fetch response".into(),
                reason: "parent returned invalid broker JSON".into(),
            })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(fetch_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent Fetch response",
                NativeWorkerFailureKind::Protocol,
                "parent Fetch response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        let document_cookie = document_cookie.to_owned();
        *self.document_cookie_projection = Some(document_cookie.clone());
        self.update_runtime_cookie_projection(&document_cookie);
        let csp_violations = response
            .get("csp_violations")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch response",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned invalid Fetch CSP violations",
                )
            })?
            .unwrap_or_default();
        let fetch = if response.get("kind").and_then(Value::as_str) == Some("error") {
            let timed_out = response
                .get("timeout")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Err(NativeEngineError::Network {
                operation: "parent-brokered script fetch".into(),
                reason: if timed_out {
                    "request timed out".into()
                } else {
                    response
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("parent rejected the network request")
                        .to_owned()
                },
            })
        } else {
            decode_fetch_response(&response, self.request_id)
        };
        Ok((fetch, document_cookie, csp_violations))
    }

    pub(crate) async fn load_worker_script(
        &mut self,
        request_id: u32,
        document_url: &str,
        href: &str,
        referrer_url: Option<&str>,
        max_source_bytes: usize,
        module_type: Option<NativeModuleResourceType>,
        credentials_mode: Option<&str>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
        shared_worker_module_entry: bool,
        entry_referrer_url: Option<&str>,
    ) -> Result<(Option<NativeScriptResource>, String), NativeEngineError> {
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "parent worker script request ID",
                "must be positive",
            ));
        }
        let cookie_writes = self.take_cookie_writes()?;
        let module_type = module_type.map(|module_type| match module_type {
            NativeModuleResourceType::JavaScript => "javascript",
            NativeModuleResourceType::Json => "json",
            NativeModuleResourceType::Unsupported => "unsupported",
        });
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": request_id,
                "document_url": document_url,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "href": href,
                "credentials": true,
                "worker_script_load": {
                    "referrer_url": referrer_url,
                    "max_source_bytes": max_source_bytes,
                    "module_type": module_type,
                    "credentials_mode": credentials_mode,
                    "referrer_policy": referrer_policy.map(NativeFetchReferrerPolicy::as_str),
                    "shared_worker_module_entry": shared_worker_module_entry,
                    "entry_referrer_url": entry_referrer_url,
                },
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent worker script broker")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent worker script response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid worker script JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(request_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent worker script response",
                NativeWorkerFailureKind::Protocol,
                "worker script response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent worker script response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        let document_cookie = document_cookie.to_owned();
        *self.document_cookie_projection = Some(document_cookie.clone());
        self.update_runtime_cookie_projection(&document_cookie);
        let resource = match response.get("kind").and_then(Value::as_str) {
            Some("worker_script_unavailable") => None,
            Some("worker_script_loaded") => {
                let url = response.get("url").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "decode parent worker script response",
                        NativeWorkerFailureKind::Protocol,
                        "parent omitted the worker script URL",
                    )
                })?;
                validate_url_text("parent worker script response URL", url)?;
                if !is_network_url(without_fragment(url)) {
                    return Err(NativeEngineError::worker_failure(
                        "decode parent worker script response",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned a non-HTTP(S) worker script URL",
                    ));
                }
                let body = response
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent worker script response",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted the worker script body",
                        )
                    })?;
                if body.len() > max_source_bytes {
                    return Err(NativeEngineError::limit(
                        "parent worker script response",
                        max_source_bytes,
                        body.len(),
                    ));
                }
                let response_referrer_policy = response
                    .get("response_referrer_policy")
                    .filter(|value| !value.is_null())
                    .map(|value| {
                        value
                            .as_str()
                            .ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "decode parent worker script response",
                                    NativeWorkerFailureKind::Protocol,
                                    "parent returned an invalid response Referrer-Policy",
                                )
                            })
                            .and_then(NativeFetchReferrerPolicy::parse)
                    })
                    .transpose()?;
                Some(NativeScriptResource {
                    url: url.to_owned(),
                    body: body.to_owned(),
                    response_referrer_policy,
                })
            }
            Some("error") => {
                return Err(NativeEngineError::Network {
                    operation: "parent-brokered worker script request".into(),
                    reason: response
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("parent rejected the worker script request")
                        .to_owned(),
                });
            }
            _ => {
                return Err(NativeEngineError::worker_failure(
                    "decode parent worker script response",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned an unknown worker script response kind",
                ));
            }
        };
        Ok((resource, document_cookie))
    }

    pub(super) async fn load_page_script(
        &mut self,
        document_url: &str,
        href: &str,
        referrer_url: Option<&str>,
        max_source_bytes: usize,
        parser_inserted: bool,
        nonce: Option<&str>,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        module_type: Option<NativeModuleResourceType>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.next_content_resource_fetch_id = self
            .next_content_resource_fetch_id
            .checked_add(1)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "parent content resource request IDs",
                    u32::MAX as usize,
                    u32::MAX as usize,
                )
            })?;
        let fetch_id = self.next_content_resource_fetch_id;
        let cookie_writes = self.take_cookie_writes()?;
        let module_type = module_type.map(|module_type| match module_type {
            NativeModuleResourceType::JavaScript => "javascript",
            NativeModuleResourceType::Json => "json",
            NativeModuleResourceType::Unsupported => "unsupported",
        });
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": fetch_id,
                "document_url": document_url,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "page_meta_csp": self.page_meta_content_security_policies,
                "href": href,
                "credentials": true,
                "page_script_load": {
                    "request_kind": match (module_type.is_some(), parser_inserted) {
                        (false, _) => "element",
                        (true, true) => "module_dependency",
                        (true, false) => "dynamic_import",
                    },
                    "referrer_url": referrer_url,
                    "max_source_bytes": max_source_bytes,
                    "parser_inserted": parser_inserted,
                    "nonce": nonce,
                    "integrity": integrity,
                    "crossorigin": crossorigin,
                    "module_type": module_type,
                    "referrer_policy": referrer_policy.map(NativeFetchReferrerPolicy::as_str),
                },
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent page script broker")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent page script response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid page script JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(fetch_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent page script response",
                NativeWorkerFailureKind::Protocol,
                "page script response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent page script response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        *self.document_cookie_projection = Some(document_cookie.to_owned());
        self.update_runtime_cookie_projection(document_cookie);
        match response.get("kind").and_then(Value::as_str) {
            Some("page_script_unavailable") => Ok(None),
            Some("page_script_loaded") => {
                let url = response.get("url").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "decode parent page script response",
                        NativeWorkerFailureKind::Protocol,
                        "parent omitted the page script URL",
                    )
                })?;
                validate_url_text("parent page script response URL", url)?;
                if !is_network_url(without_fragment(url)) {
                    return Err(NativeEngineError::worker_failure(
                        "decode parent page script response",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned a non-HTTP(S) page script URL",
                    ));
                }
                let body = response
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent page script response",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted the page script body",
                        )
                    })?;
                if body.len() > max_source_bytes {
                    return Err(NativeEngineError::limit(
                        "parent page script response",
                        max_source_bytes,
                        body.len(),
                    ));
                }
                let response_referrer_policy = match response
                    .get("response_referrer_policy")
                    .and_then(Value::as_str)
                {
                    Some(policy) => Some(NativeFetchReferrerPolicy::parse(policy)?),
                    None => None,
                };
                Ok(Some(NativeScriptResource {
                    url: url.to_owned(),
                    body: body.to_owned(),
                    response_referrer_policy,
                }))
            }
            Some("error") => Err(NativeEngineError::Network {
                operation: "parent-brokered page script load".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the page script request")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent page script response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown page script response kind",
            )),
        }
    }

    pub(super) async fn load_stylesheet(
        &mut self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeStylesheetResource>, NativeEngineError> {
        self.next_content_resource_fetch_id = self
            .next_content_resource_fetch_id
            .checked_add(1)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "parent content resource request IDs",
                    u32::MAX as usize,
                    u32::MAX as usize,
                )
            })?;
        let fetch_id = self.next_content_resource_fetch_id;
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": fetch_id,
                "document_url": document_url,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "page_meta_csp": self.page_meta_content_security_policies,
                "href": href,
                "credentials": true,
                "page_stylesheet_load": {
                    "integrity": integrity,
                    "crossorigin": crossorigin,
                    "referrer_policy": referrer_policy.map(NativeFetchReferrerPolicy::as_str),
                    "max_body_bytes": MAX_CONTENT_STYLESHEET_BYTES,
                },
            }),
        )
        .await?;
        let payload = self
            .receive_parent_response("parent stylesheet broker")
            .await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent stylesheet response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid stylesheet JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(fetch_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent stylesheet response",
                NativeWorkerFailureKind::Protocol,
                "stylesheet response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent stylesheet response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        *self.document_cookie_projection = Some(document_cookie.to_owned());
        self.update_runtime_cookie_projection(document_cookie);
        match response.get("kind").and_then(Value::as_str) {
            Some("page_stylesheet_unavailable") => Ok(None),
            Some("page_stylesheet_loaded") => {
                let url = response.get("url").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "decode parent stylesheet response",
                        NativeWorkerFailureKind::Protocol,
                        "parent omitted the stylesheet URL",
                    )
                })?;
                validate_url_text("parent stylesheet response URL", url)?;
                if !is_network_url(without_fragment(url)) {
                    return Err(NativeEngineError::worker_failure(
                        "decode parent stylesheet response",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned a non-HTTP(S) stylesheet URL",
                    ));
                }
                let body = response
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent stylesheet response",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted the stylesheet body",
                        )
                    })?;
                if body.len() > MAX_CONTENT_STYLESHEET_BYTES {
                    return Err(NativeEngineError::limit(
                        "parent stylesheet response",
                        MAX_CONTENT_STYLESHEET_BYTES,
                        body.len(),
                    ));
                }
                Ok(Some(NativeStylesheetResource {
                    url: url.to_owned(),
                    body: body.to_owned(),
                }))
            }
            Some("error") => Err(NativeEngineError::Network {
                operation: "parent-brokered stylesheet load".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the stylesheet request")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent stylesheet response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown stylesheet response kind",
            )),
        }
    }

    pub(super) async fn load_image(
        &mut self,
        document_url: &str,
        href: &str,
        referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        self.load_image_with_metadata(
            document_url,
            href,
            json!({
                "referrer_policy": referrer_policy.as_str(),
            }),
        )
        .await
    }

    pub(super) async fn load_video_poster(
        &mut self,
        document_url: &str,
        href: &str,
        referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        self.load_image_with_metadata(
            document_url,
            href,
            json!({
                "client": "document",
                "destination": "image",
                "initiator_type": "video",
                "credentials_mode": "include",
                "use_url_credentials": true,
                "referrer_policy": referrer_policy.as_str(),
            }),
        )
        .await
    }

    pub(super) async fn load_image_with_metadata(
        &mut self,
        document_url: &str,
        href: &str,
        page_image_load: Value,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        self.next_content_resource_fetch_id = self
            .next_content_resource_fetch_id
            .checked_add(1)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "parent content resource request IDs",
                    u32::MAX as usize,
                    u32::MAX as usize,
                )
            })?;
        let fetch_id = self.next_content_resource_fetch_id;
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": fetch_id,
                "document_url": document_url,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "page_meta_csp": self.page_meta_content_security_policies,
                "href": href,
                "credentials": true,
                "page_image_load": page_image_load,
            }),
        )
        .await?;
        let payload = self.receive_parent_response("parent image broker").await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent image response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid image JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(fetch_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent image response",
                NativeWorkerFailureKind::Protocol,
                "image response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent image response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        *self.document_cookie_projection = Some(document_cookie.to_owned());
        self.update_runtime_cookie_projection(document_cookie);
        match response.get("kind").and_then(Value::as_str) {
            Some("page_image_unavailable") => Ok(None),
            Some("page_image_loaded") => {
                let value = response.get("image").cloned().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "decode parent image response",
                        NativeWorkerFailureKind::Protocol,
                        "parent omitted the image resource",
                    )
                })?;
                let resource: NativeImageResourceWire =
                    serde_json::from_value(value).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "decode parent image response",
                            NativeWorkerFailureKind::Protocol,
                            "parent returned malformed image metadata",
                        )
                    })?;
                if resource.node_index != 0 || resource.source != href {
                    return Err(NativeEngineError::worker_failure(
                        "decode parent image response",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned an image for a different resource",
                    ));
                }
                let max_encoded_bytes = MAX_NATIVE_IMAGE_TRANSFER_BYTES
                    .saturating_add(2)
                    .saturating_div(3)
                    .saturating_mul(4);
                image_from_wire(&resource, max_encoded_bytes).map(Some)
            }
            Some("error") => Err(NativeEngineError::Network {
                operation: "parent-brokered image load".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the image request")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent image response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown image response kind",
            )),
        }
    }

    pub(super) async fn load_media(
        &mut self,
        document_url: &str,
        policy_document_url: &str,
        base_url: &str,
        href: &str,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        self.next_content_resource_fetch_id = self
            .next_content_resource_fetch_id
            .checked_add(1)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "parent content resource request IDs",
                    u32::MAX as usize,
                    u32::MAX as usize,
                )
            })?;
        let fetch_id = self.next_content_resource_fetch_id;
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": fetch_id,
                "document_url": document_url,
                "policy_document_url": policy_document_url,
                "base_url": base_url,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "page_meta_csp": self.page_meta_content_security_policies,
                "href": href,
                "credentials": true,
                "page_media_load": {},
            }),
        )
        .await?;
        let payload = self.receive_parent_response("parent media broker").await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent media response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid media JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(fetch_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent media response",
                NativeWorkerFailureKind::Protocol,
                "media response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent media response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        *self.document_cookie_projection = Some(document_cookie.to_owned());
        self.update_runtime_cookie_projection(document_cookie);
        match response.get("kind").and_then(Value::as_str) {
            Some("page_media_unavailable") => Ok(None),
            Some("page_media_loaded") => {
                let metadata = serde_json::from_value::<NativeMediaMetadata>(
                    response.get("metadata").cloned().ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent media response",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted media metadata",
                        )
                    })?,
                )
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "decode parent media response",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned malformed media metadata",
                    )
                })?;
                if metadata.content_type.is_empty()
                    || metadata.content_type.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES
                    || metadata.byte_length == 0
                    || metadata.byte_length > MAX_NATIVE_MEDIA_BYTES
                {
                    return Err(NativeEngineError::worker_failure(
                        "decode parent media response",
                        NativeWorkerFailureKind::Protocol,
                        "parent returned out-of-range media metadata",
                    ));
                }
                Ok(Some(metadata))
            }
            Some("error") => Err(NativeEngineError::Network {
                operation: "parent-brokered media load".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the media request")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent media response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown media response kind",
            )),
        }
    }

    pub(super) async fn load_font_response(
        &mut self,
        document_url: &str,
        href: &str,
    ) -> Result<Option<NativeFetchResponse>, NativeEngineError> {
        self.next_content_resource_fetch_id = self
            .next_content_resource_fetch_id
            .checked_add(1)
            .ok_or_else(|| {
                NativeEngineError::limit(
                    "parent content resource request IDs",
                    u32::MAX as usize,
                    u32::MAX as usize,
                )
            })?;
        let fetch_id = self.next_content_resource_fetch_id;
        let cookie_writes = self.take_cookie_writes()?;
        write_value_frame(
            self.stdout,
            &json!({
                "kind": "parent_fetch_request",
                "id": self.request_id,
                "fetch_id": fetch_id,
                "document_url": document_url,
                "owner": self.owner,
                "cookie_writes": cookie_writes,
                "page_meta_csp": self.page_meta_content_security_policies,
                "href": href,
                "credentials": true,
                "page_font_load": {},
            }),
        )
        .await?;
        let payload = self.receive_parent_response("parent font broker").await?;
        let response: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                "decode parent font response",
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid font JSON",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(self.request_id)
            || response.get("fetch_id").and_then(Value::as_u64) != Some(u64::from(fetch_id))
        {
            return Err(NativeEngineError::worker_failure(
                "decode parent font response",
                NativeWorkerFailureKind::Protocol,
                "font response belongs to a different request",
            ));
        }
        let document_cookie = response
            .get("document_cookie")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "decode parent font response",
                    NativeWorkerFailureKind::Protocol,
                    "parent omitted the document cookie projection",
                )
            })?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        *self.document_cookie_projection = Some(document_cookie.to_owned());
        self.update_runtime_cookie_projection(document_cookie);
        match response.get("kind").and_then(Value::as_str) {
            Some("page_font_unavailable") => Ok(None),
            Some("page_font_loaded") => {
                let max_encoded_body_bytes = MAX_NATIVE_FONT_BYTES
                    .saturating_add(2)
                    .saturating_div(3)
                    .saturating_mul(4);
                let encoded_body_bytes = response
                    .get("body_base64")
                    .and_then(Value::as_str)
                    .map(str::len)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "decode parent font response",
                            NativeWorkerFailureKind::Protocol,
                            "parent omitted the font body",
                        )
                    })?;
                if encoded_body_bytes > max_encoded_body_bytes {
                    return Err(NativeEngineError::limit(
                        "parent font response",
                        max_encoded_body_bytes,
                        encoded_body_bytes,
                    ));
                }
                let mut fetch_response = response.clone();
                fetch_response["kind"] = Value::String("fetched".into());
                let fetch_response = decode_fetch_response(&fetch_response, self.request_id)?;
                if fetch_response.body.len() > MAX_NATIVE_FONT_BYTES {
                    return Err(NativeEngineError::limit(
                        "parent font response",
                        MAX_NATIVE_FONT_BYTES,
                        fetch_response.body.len(),
                    ));
                }
                Ok(Some(fetch_response))
            }
            Some("error") => Err(NativeEngineError::Network {
                operation: "parent-brokered font load".into(),
                reason: response
                    .get("reason")
                    .or_else(|| response.get("error"))
                    .and_then(Value::as_str)
                    .unwrap_or("parent rejected the font request")
                    .to_owned(),
            }),
            _ => Err(NativeEngineError::worker_failure(
                "decode parent font response",
                NativeWorkerFailureKind::Protocol,
                "parent returned an unknown font response kind",
            )),
        }
    }
}
