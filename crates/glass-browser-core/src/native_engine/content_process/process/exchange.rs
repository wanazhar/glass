use super::*;

impl NativeContentProcess {
    pub(super) async fn exchange_inner(
        &mut self,
        request: Value,
        mut parent_loader: Option<&mut NativeResourceLoader>,
    ) -> Result<Value, NativeEngineError> {
        let request_id = request.get("id").and_then(Value::as_u64).ok_or_else(|| {
            NativeEngineError::invalid("content IPC request ID", "must be an unsigned integer")
        })?;
        let payload = serde_json::to_vec(&request).map_err(|_| NativeEngineError::Worker {
            operation: "encode content IPC".into(),
            reason: "content process request could not be encoded".into(),
        })?;
        write_frame(&mut self.stdin, &payload).await?;
        let mut resumed_dialog: Option<NativeDialogWait> = None;
        let mut parent_navigation_document_url: Option<String> = None;
        let mut cancellable_parent_navigation_fetches = BTreeSet::new();
        loop {
            let response = self.read_response_value().await?;
            if response.get("kind").and_then(Value::as_str) == Some("parent_fetch_cancel") {
                let fetch_id = decode_parent_fetch_cancellation(
                    &response,
                    request_id,
                    &mut cancellable_parent_navigation_fetches,
                )?;
                self.acknowledge_parent_fetch_cancellation(request_id, fetch_id)
                    .await?;
                continue;
            }
            if response.get("kind").and_then(Value::as_str) == Some("parent_navigation_request") {
                if let Some(document_url) = Box::pin(self.handle_parent_navigation_request(
                    request_id,
                    &request,
                    &response,
                    parent_loader.as_deref_mut(),
                ))
                .await?
                {
                    parent_navigation_document_url = Some(document_url);
                }
                continue;
            }
            if response
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.starts_with("parent_websocket_"))
            {
                let payload_value = Box::pin(self.handle_parent_websocket_request(
                    &request,
                    &response,
                    parent_navigation_document_url.as_deref(),
                    parent_loader.as_deref_mut(),
                ))
                .await?;
                let payload = serde_json::to_vec(&payload_value).map_err(|_| {
                    NativeEngineError::worker_failure(
                        "encode parent WebSocket response",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket response could not be encoded",
                    )
                })?;
                write_frame(&mut self.stdin, &payload).await?;
                continue;
            }
            if response
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.starts_with("parent_event_source_"))
            {
                let loader = parent_loader.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process EventSource broker",
                        NativeWorkerFailureKind::Protocol,
                        "content process requested parent EventSource service without a loader",
                    )
                })?;
                if response.get("id").and_then(Value::as_u64) != Some(request_id) {
                    return Err(NativeEngineError::worker_failure(
                        "content process EventSource broker",
                        NativeWorkerFailureKind::Protocol,
                        "EventSource request belongs to a different content operation",
                    ));
                }
                let kind = response
                    .get("kind")
                    .and_then(Value::as_str)
                    .expect("matched EventSource request kind");
                let source_id = response
                    .get("source_id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value != 0)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process EventSource broker",
                            NativeWorkerFailureKind::Protocol,
                            "EventSource request has an invalid source ID",
                        )
                    })?;
                let worker_id = decode_optional_content_worker_id(
                    &response,
                    "content process EventSource worker ID",
                )?;
                let owner = decode_content_cookie_owner(
                    response.get("owner").ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process EventSource broker",
                            NativeWorkerFailureKind::Protocol,
                            "EventSource request omitted its owner",
                        )
                    })?,
                    "content process EventSource cookie owner",
                )?;
                let expected_owner = request
                    .get("owner")
                    .map(|value| decode_content_cookie_owner(value, "parent script owner"))
                    .transpose()?;
                if expected_owner
                    .as_ref()
                    .is_some_and(|expected_owner| expected_owner != &owner)
                    || !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    )
                {
                    return Err(NativeEngineError::worker_failure(
                        "content process EventSource broker",
                        NativeWorkerFailureKind::Protocol,
                        "EventSource request does not match the active document owner",
                    ));
                }
                let writes = serde_json::from_value::<Vec<NativeContentCookieWrite>>(
                    response.get("cookie_writes").cloned().ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process EventSource broker",
                            NativeWorkerFailureKind::Protocol,
                            "EventSource request omitted its cookie-write journal",
                        )
                    })?,
                )
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "content process EventSource broker",
                        NativeWorkerFailureKind::Protocol,
                        "EventSource cookie-write journal is malformed",
                    )
                })?;
                if writes.len() > MAX_NATIVE_EFFECTS {
                    return Err(NativeEngineError::limit(
                        "parent-owned document.cookie writes",
                        MAX_NATIVE_EFFECTS,
                        writes.len(),
                    ));
                }
                let mut write_bytes = 0usize;
                for write in &writes {
                    if write.owner != owner || write.value.len() > crate::limits::MAX_TEXT_BYTES {
                        return Err(NativeEngineError::worker_failure(
                            "content process EventSource broker",
                            NativeWorkerFailureKind::Protocol,
                            "EventSource cookie write is oversized or belongs to another owner",
                        ));
                    }
                    write_bytes = write_bytes.saturating_add(write.value.len());
                }
                if write_bytes > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
                    return Err(NativeEngineError::limit(
                        "parent-owned document.cookie write journal",
                        MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                        write_bytes,
                    ));
                }
                for write in &writes {
                    loader.set_document_cookie(&owner.document_url, &write.value)?;
                }
                let document_cookie = loader.document_cookie(&owner.document_url)?;
                let stream_id = response
                    .get("stream_id")
                    .and_then(Value::as_u64)
                    .filter(|stream_id| *stream_id > 0);
                let payload_value = match kind {
                    "parent_event_source_open_request" => {
                        if response.get("stream_id").is_some() {
                            return Err(NativeEngineError::worker_failure(
                                "content process EventSource open",
                                NativeWorkerFailureKind::Protocol,
                                "EventSource open request carried a stream ID",
                            ));
                        }
                        let document_url = response
                            .get("document_url")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "content process EventSource open",
                                    NativeWorkerFailureKind::Protocol,
                                    "EventSource open omitted its document URL",
                                )
                            })?;
                        let initiator_url = response
                            .get("initiator_url")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process EventSource open",
                                NativeWorkerFailureKind::Protocol,
                                "EventSource open omitted its initiator URL",
                            )
                        })?;
                        let href =
                            response
                                .get("href")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    NativeEngineError::worker_failure(
                                        "content process EventSource open",
                                        NativeWorkerFailureKind::Protocol,
                                        "EventSource open omitted its target URL",
                                    )
                                })?;
                        let with_credentials = response
                            .get("with_credentials")
                            .and_then(Value::as_bool)
                            .ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "content process EventSource open",
                                    NativeWorkerFailureKind::Protocol,
                                    "EventSource open omitted its credentials mode",
                                )
                            })?;
                        let last_event_id = response
                            .get("last_event_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process EventSource open",
                                NativeWorkerFailureKind::Protocol,
                                "EventSource open omitted its last event ID",
                            )
                        })?;
                        if without_fragment(document_url) != without_fragment(&owner.document_url) {
                            return Err(NativeEngineError::worker_failure(
                                "content process EventSource open",
                                NativeWorkerFailureKind::Protocol,
                                "EventSource document URL does not match its owner",
                            ));
                        }
                        if worker_id.is_none()
                            && without_fragment(initiator_url)
                                != without_fragment(&owner.document_url)
                        {
                            return Err(NativeEngineError::worker_failure(
                                "content process EventSource open",
                                NativeWorkerFailureKind::Protocol,
                                "page EventSource initiator does not match its owner",
                            ));
                        }
                        if self.parent_event_source_streams.len()
                            >= MAX_NATIVE_EVENTSOURCE_CONNECTIONS
                        {
                            json!({
                                "kind": "parent_event_source_error",
                                "id": request_id,
                                "source_id": source_id,
                                "reason": "parent EventSource stream limit reached",
                                "document_cookie": document_cookie,
                                "csp_violations": [],
                            })
                        } else {
                            match loader
                                .open_event_source_async(
                                    initiator_url,
                                    href,
                                    with_credentials,
                                    last_event_id,
                                )
                                .await
                            {
                                Ok((url, stream)) => {
                                    let stream_id = self.next_parent_event_source_stream_id;
                                    self.next_parent_event_source_stream_id =
                                        stream_id.checked_add(1).ok_or_else(|| {
                                            NativeEngineError::limit(
                                                "parent EventSource stream IDs",
                                                u64::MAX as usize,
                                                usize::MAX,
                                            )
                                        })?;
                                    let csp_violations = loader.take_csp_violations();
                                    self.parent_event_source_streams.insert(
                                        stream_id,
                                        NativeParentEventSourceStream {
                                            owner: owner.clone(),
                                            source_id,
                                            worker_id,
                                            response: stream,
                                        },
                                    );
                                    json!({
                                        "kind": "parent_event_source_opened",
                                        "id": request_id,
                                        "source_id": source_id,
                                        "stream_id": stream_id,
                                        "origin": url.origin().ascii_serialization(),
                                        "document_cookie": loader.document_cookie(&owner.document_url)?,
                                        "csp_violations": csp_violations,
                                    })
                                }
                                Err(error) => json!({
                                    "kind": "parent_event_source_error",
                                    "id": request_id,
                                    "source_id": source_id,
                                    "reason": bounded_websocket_text(
                                        error.to_string(),
                                        MAX_NATIVE_SCRIPT_BYTES,
                                    ),
                                    "document_cookie": loader.document_cookie(&owner.document_url)?,
                                    "csp_violations": loader.take_csp_violations(),
                                }),
                            }
                        }
                    }
                    "parent_event_source_read_request" => {
                        let stream_id = stream_id.ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process EventSource read",
                                NativeWorkerFailureKind::Protocol,
                                "EventSource read has an invalid stream ID",
                            )
                        })?;
                        let read_result = if let Some(stream) =
                            self.parent_event_source_streams.get_mut(&stream_id)
                        {
                            if stream.owner != owner
                                || stream.source_id != source_id
                                || stream.worker_id != worker_id
                            {
                                return Err(NativeEngineError::worker_failure(
                                    "content process EventSource read",
                                    NativeWorkerFailureKind::Protocol,
                                    "EventSource stream does not belong to the requesting owner",
                                ));
                            }
                            Some(
                                timeout(NATIVE_WEBSOCKET_POLL_INTERVAL, stream.response.chunk())
                                    .await
                                    .map(|result| {
                                        result.map(|chunk| chunk.map(|chunk| chunk.to_vec()))
                                    }),
                            )
                        } else {
                            None
                        };
                        match read_result {
                            None => json!({
                                "kind": "parent_event_source_closed",
                                "id": request_id,
                                "source_id": source_id,
                                "stream_id": stream_id,
                                "reason": "parent EventSource stream is no longer active",
                                "document_cookie": document_cookie,
                            }),
                            Some(Ok(Ok(Some(chunk)))) => {
                                let maximum_chunk = MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES
                                    + MAX_NATIVE_EVENTSOURCE_FIELD_BYTES;
                                if chunk.len() > maximum_chunk {
                                    self.parent_event_source_streams.remove(&stream_id);
                                    json!({
                                        "kind": "parent_event_source_error",
                                        "id": request_id,
                                        "source_id": source_id,
                                        "stream_id": stream_id,
                                        "reason": "parent EventSource chunk exceeds the bounded IPC limit",
                                        "document_cookie": document_cookie,
                                    })
                                } else {
                                    json!({
                                        "kind": "parent_event_source_chunk",
                                        "id": request_id,
                                        "source_id": source_id,
                                        "stream_id": stream_id,
                                        "chunk_base64": base64::engine::general_purpose::STANDARD.encode(chunk),
                                        "document_cookie": document_cookie,
                                    })
                                }
                            }
                            Some(Ok(Ok(None))) => {
                                self.parent_event_source_streams.remove(&stream_id);
                                json!({
                                    "kind": "parent_event_source_end",
                                    "id": request_id,
                                    "source_id": source_id,
                                    "stream_id": stream_id,
                                    "document_cookie": document_cookie,
                                })
                            }
                            Some(Ok(Err(error))) => {
                                self.parent_event_source_streams.remove(&stream_id);
                                json!({
                                    "kind": "parent_event_source_error",
                                    "id": request_id,
                                    "source_id": source_id,
                                    "stream_id": stream_id,
                                    "reason": bounded_websocket_text(
                                        error.to_string(),
                                        MAX_NATIVE_SCRIPT_BYTES,
                                    ),
                                    "document_cookie": document_cookie,
                                })
                            }
                            Some(Err(_)) => json!({
                                "kind": "parent_event_source_pending",
                                "id": request_id,
                                "source_id": source_id,
                                "stream_id": stream_id,
                                "document_cookie": document_cookie,
                            }),
                        }
                    }
                    "parent_event_source_close_request" => {
                        let stream_id = stream_id.ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process EventSource close",
                                NativeWorkerFailureKind::Protocol,
                                "EventSource close has an invalid stream ID",
                            )
                        })?;
                        match self.parent_event_source_streams.get(&stream_id) {
                            Some(stream)
                                if stream.owner == owner
                                    && stream.source_id == source_id
                                    && stream.worker_id == worker_id =>
                            {
                                self.parent_event_source_streams.remove(&stream_id);
                                json!({
                                    "kind": "parent_event_source_closed",
                                    "id": request_id,
                                    "source_id": source_id,
                                    "stream_id": stream_id,
                                    "document_cookie": document_cookie,
                                })
                            }
                            Some(_) => {
                                return Err(NativeEngineError::worker_failure(
                                    "content process EventSource close",
                                    NativeWorkerFailureKind::Protocol,
                                    "EventSource close does not match the stream owner",
                                ));
                            }
                            None => json!({
                                "kind": "parent_event_source_closed",
                                "id": request_id,
                                "source_id": source_id,
                                "stream_id": stream_id,
                                "document_cookie": document_cookie,
                            }),
                        }
                    }
                    _ => {
                        return Err(NativeEngineError::worker_failure(
                            "content process EventSource broker",
                            NativeWorkerFailureKind::Protocol,
                            "content process sent an unknown parent EventSource operation",
                        ));
                    }
                };
                let payload = serde_json::to_vec(&payload_value).map_err(|_| {
                    NativeEngineError::worker_failure(
                        "encode parent EventSource response",
                        NativeWorkerFailureKind::Protocol,
                        "EventSource response could not be encoded",
                    )
                })?;
                write_frame(&mut self.stdin, &payload).await?;
                continue;
            }
            if response.get("kind").and_then(Value::as_str) == Some("parent_sync_xhr_request") {
                let loader = parent_loader.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process parent synchronous XHR broker",
                        NativeWorkerFailureKind::Protocol,
                        "content process requested synchronous XHR without a parent network broker",
                    )
                })?;
                if response.get("id").and_then(Value::as_u64) != Some(request_id) {
                    return Err(NativeEngineError::worker_failure(
                        "content process parent synchronous XHR broker",
                        NativeWorkerFailureKind::Protocol,
                        "synchronous XHR belongs to a different content operation",
                    ));
                }
                let sync_id = response
                    .get("sync_id")
                    .and_then(Value::as_u64)
                    .filter(|sync_id| *sync_id != 0)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent synchronous XHR broker",
                            NativeWorkerFailureKind::Protocol,
                            "synchronous XHR has an invalid request ID",
                        )
                    })?;
                let owner = decode_content_cookie_owner(
                    response.get("owner").ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent synchronous XHR broker",
                            NativeWorkerFailureKind::Protocol,
                            "synchronous XHR omitted its captured cookie owner",
                        )
                    })?,
                    "content process parent synchronous XHR owner",
                )?;
                let expected_owner = request
                    .get("owner")
                    .map(|value| decode_content_cookie_owner(value, "active synchronous XHR owner"))
                    .transpose()?;
                if expected_owner
                    .as_ref()
                    .is_some_and(|expected| expected != &owner)
                    || !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    )
                {
                    return Err(NativeEngineError::worker_failure(
                        "content process parent synchronous XHR broker",
                        NativeWorkerFailureKind::Protocol,
                        "synchronous XHR owner does not match the active document",
                    ));
                }
                let xhr = response.get("request").ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process parent synchronous XHR broker",
                        NativeWorkerFailureKind::Protocol,
                        "synchronous XHR omitted request metadata",
                    )
                })?;
                let document_url =
                    xhr.get("documentUrl")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent synchronous XHR broker",
                                NativeWorkerFailureKind::Protocol,
                                "synchronous XHR omitted its initiator URL",
                            )
                        })?;
                let href = xhr.get("href").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process parent synchronous XHR broker",
                        NativeWorkerFailureKind::Protocol,
                        "synchronous XHR omitted its target URL",
                    )
                })?;
                let credentials =
                    xhr.get("credentials")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent synchronous XHR broker",
                                NativeWorkerFailureKind::Protocol,
                                "synchronous XHR omitted its credentials mode",
                            )
                        })?;
                let body = match xhr.get("bodyBase64") {
                    None | Some(Value::Null) => Value::Null,
                    Some(Value::String(encoded)) => json!({
                        "kind": "bytes",
                        "value": encoded,
                    }),
                    Some(_) => {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent synchronous XHR broker",
                            NativeWorkerFailureKind::Protocol,
                            "synchronous XHR body is malformed",
                        ));
                    }
                };
                let fetch_request = json!({
                    "method": xhr.get("method"),
                    "body": body,
                    "content_type": xhr.get("contentType"),
                    "request_headers": xhr.get("headers"),
                    "cors_mode": "cors",
                    "redirect_mode": "follow",
                    "cache_mode": "default",
                    "timeout_ms": xhr.get("timeoutMs"),
                    "referrer_url": null,
                    "referrer_policy": xhr.get("referrerPolicy"),
                    "credentials_mode": NativeFetchCredentialsMode::for_xhr(credentials).as_str(),
                    "max_response_bytes": null,
                });
                let request =
                    decode_parent_fetch_request(document_url, href, credentials, &fetch_request)?;
                let cookie_write_values = serde_json::from_value::<Vec<String>>(
                    response.get("cookie_writes").cloned().ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent synchronous XHR broker",
                            NativeWorkerFailureKind::Protocol,
                            "synchronous XHR omitted its cookie-write journal",
                        )
                    })?,
                )
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "content process parent synchronous XHR broker",
                        NativeWorkerFailureKind::Protocol,
                        "synchronous XHR cookie-write journal is malformed",
                    )
                })?;
                let writes = cookie_write_values
                    .into_iter()
                    .map(|value| NativeContentCookieWrite {
                        owner: owner.clone(),
                        value,
                    })
                    .collect::<Vec<_>>();
                let cookie_writes = json!({ "cookie_writes": writes });
                apply_parent_content_cookie_writes(
                    loader,
                    &owner,
                    &cookie_writes,
                    "parent synchronous XHR cookie-write journal",
                )?;
                let fetch = loader.fetch_request_with_headers_async(request).await;
                let document_cookie = loader.document_cookie(&owner.document_url)?;
                let response = json!({
                    "kind": "parent_sync_xhr_response",
                    "id": request_id,
                    "sync_id": sync_id,
                    "document_cookie": document_cookie,
                    "fetch": parent_fetch_response_payload(request_id, fetch),
                });
                let payload = serde_json::to_vec(&response).map_err(|_| {
                    NativeEngineError::worker_failure(
                        "encode parent synchronous XHR response",
                        NativeWorkerFailureKind::Protocol,
                        "synchronous XHR response could not be encoded",
                    )
                })?;
                write_frame(&mut self.stdin, &payload).await?;
                continue;
            }
            if response.get("kind").and_then(Value::as_str) == Some("parent_fetch_request") {
                let loader = parent_loader.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "content process parent Fetch broker",
                        NativeWorkerFailureKind::Protocol,
                        "content process requested parent network access without a broker",
                    )
                })?;
                if response.get("id").and_then(Value::as_u64) != Some(request_id) {
                    return Err(NativeEngineError::worker_failure(
                        "content process parent Fetch broker",
                        NativeWorkerFailureKind::Protocol,
                        "parent Fetch request belongs to a different content operation",
                    ));
                }
                let document_url = response
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "parent Fetch request omitted its document URL",
                        )
                    })?;
                let href = response
                    .get("href")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "parent Fetch request omitted its target URL",
                        )
                    })?;
                let credentials = response
                    .get("credentials")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "parent Fetch request omitted its credentials mode",
                        )
                    })?;
                let fetch_id = response
                    .get("fetch_id")
                    .map(|value| {
                        value
                            .as_u64()
                            .and_then(|value| u32::try_from(value).ok())
                            .filter(|value| *value != 0)
                            .ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "content process parent Fetch broker",
                                    NativeWorkerFailureKind::Protocol,
                                    "parent Fetch request has an invalid script request ID",
                                )
                            })
                    })
                    .transpose()?;
                let captured_load_fetch = response
                    .get("captured_load_fetch")
                    .map(|value| {
                        value.as_bool().ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent Fetch broker",
                                NativeWorkerFailureKind::Protocol,
                                "captured-load Fetch marker must be boolean",
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or(false);
                let cookie_writes = if fetch_id.is_some() {
                    let owner_value = response.get("owner").ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "script Fetch omitted its captured owner",
                        )
                    })?;
                    let owner = decode_content_cookie_owner(
                        owner_value,
                        "content process parent Fetch broker owner",
                    )?;
                    let expected_owner = request
                        .get("owner")
                        .map(|value| {
                            decode_content_cookie_owner(value, "parent script request owner")
                        })
                        .transpose()?;
                    let is_resource_load = response.get("page_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_media_load").is_some()
                        || response.get("page_font_load").is_some()
                        || response.get("worker_script_load").is_some()
                        || captured_load_fetch;
                    if expected_owner
                        .as_ref()
                        .is_some_and(|expected_owner| expected_owner != &owner)
                        || (expected_owner.is_none() && !is_resource_load)
                        || !parent_broker_owner_matches_operation(
                            self,
                            &request,
                            &owner,
                            parent_navigation_document_url.as_deref(),
                        )
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "script request owner does not match the active document or captured load",
                        ));
                    }
                    let is_page_resource_load = response.get("page_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_media_load").is_some()
                        || response.get("page_font_load").is_some();
                    let page_meta_csp = response
                        .get("page_meta_csp")
                        .filter(|value| !value.is_null());
                    if is_page_resource_load || page_meta_csp.is_some() {
                        if !is_page_resource_load
                            && without_fragment(document_url)
                                != without_fragment(&owner.document_url)
                        {
                            return Err(NativeEngineError::worker_failure(
                                "content process parent Fetch broker",
                                NativeWorkerFailureKind::Protocol,
                                "page CSP metadata belongs to a different Fetch owner",
                            ));
                        }
                        let policies = match page_meta_csp {
                            Some(value) => decode_parent_page_meta_csp(value)?,
                            None => {
                                return Err(NativeEngineError::worker_failure(
                                    "content process parent Fetch broker",
                                    NativeWorkerFailureKind::Protocol,
                                    "page resource load omitted its CSP metadata",
                                ));
                            }
                        };
                        self.apply_parent_page_meta_csp(loader, &owner, &policies)?;
                    }
                    let raw_writes = response.get("cookie_writes").ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "script Fetch omitted its cookie-write journal",
                        )
                    })?;
                    let writes =
                        serde_json::from_value::<Vec<NativeContentCookieWrite>>(raw_writes.clone())
                            .map_err(|_| {
                                NativeEngineError::worker_failure(
                                    "content process parent Fetch broker",
                                    NativeWorkerFailureKind::Protocol,
                                    "script Fetch cookie-write journal is malformed",
                                )
                            })?;
                    if writes.len() > MAX_NATIVE_EFFECTS {
                        return Err(NativeEngineError::limit(
                            "parent-owned document.cookie writes",
                            MAX_NATIVE_EFFECTS,
                            writes.len(),
                        ));
                    }
                    let mut total_write_bytes = 0usize;
                    for write in &writes {
                        if write.owner != owner {
                            return Err(NativeEngineError::worker_failure(
                                "content process parent Fetch broker",
                                NativeWorkerFailureKind::Protocol,
                                "cookie write belongs to a different document owner",
                            ));
                        }
                        if write.value.len() > crate::limits::MAX_TEXT_BYTES {
                            return Err(NativeEngineError::limit(
                                "parent-owned document.cookie write",
                                crate::limits::MAX_TEXT_BYTES,
                                write.value.len(),
                            ));
                        }
                        total_write_bytes = total_write_bytes.saturating_add(write.value.len());
                    }
                    if total_write_bytes > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
                        return Err(NativeEngineError::limit(
                            "parent-owned document.cookie write journal",
                            MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                            total_write_bytes,
                        ));
                    }
                    writes
                } else {
                    if self.current_document_url.as_deref().is_none_or(|current| {
                        without_fragment(current) != without_fragment(document_url)
                    }) {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "parent Fetch request does not match the committed document",
                        ));
                    }
                    if response.get("owner").is_some()
                        || response.get("cookie_writes").is_some()
                        || captured_load_fetch
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "non-script Fetch carried script cookie ownership data",
                        ));
                    }
                    if response
                        .get("page_meta_csp")
                        .is_some_and(|value| !value.is_null())
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "unowned Fetch carried page CSP metadata",
                        ));
                    }
                    Vec::new()
                };
                if let Some(worker_script_load) = response.get("worker_script_load") {
                    let Some(fetch_id) = fetch_id else {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "worker script request omitted its owner-bound request ID",
                        ));
                    };
                    if response.get("fetch_request").is_some()
                        || response.get("page_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_media_load").is_some()
                        || response.get("page_font_load").is_some()
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "worker script request also carried another load type",
                        ));
                    }
                    let owner = decode_content_cookie_owner(
                        response.get("owner").ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent worker script broker",
                                NativeWorkerFailureKind::Protocol,
                                "worker script request omitted its owner",
                            )
                        })?,
                        "content process parent worker script owner",
                    )?;
                    if !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    ) {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent worker script broker",
                            NativeWorkerFailureKind::Protocol,
                            "worker script owner does not match the active document or captured load",
                        ));
                    }
                    if self.parent_brokered_script_owner.as_ref() != Some(&owner) {
                        self.parent_brokered_script_owner = Some(owner.clone());
                        self.parent_brokered_script_urls.clear();
                    }
                    let owner_document_url = without_fragment(&owner.document_url);
                    let requested_document_url = without_fragment(document_url);
                    if requested_document_url != owner_document_url
                        && !self
                            .parent_brokered_script_urls
                            .contains(requested_document_url)
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent worker script broker",
                            NativeWorkerFailureKind::Protocol,
                            "worker script request document is not owned by the active page",
                        ));
                    }
                    let referrer_url = match worker_script_load.get("referrer_url") {
                        None | Some(Value::Null) => None,
                        Some(value) => Some(value.as_str().ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent worker script broker",
                                NativeWorkerFailureKind::Protocol,
                                "worker script request has an invalid referrer URL",
                            )
                        })?),
                    };
                    if let Some(referrer_url) = referrer_url {
                        validate_url_text("worker script request referrer URL", referrer_url)?;
                        let referrer_url = without_fragment(referrer_url);
                        if referrer_url != owner_document_url
                            && !self.parent_brokered_script_urls.contains(referrer_url)
                        {
                            return Err(NativeEngineError::worker_failure(
                                "content process parent worker script broker",
                                NativeWorkerFailureKind::Protocol,
                                "worker script referrer is not owned by the active page",
                            ));
                        }
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    let mut resource = load_parent_worker_script_async(
                        loader,
                        document_url,
                        href,
                        worker_script_load,
                    )
                    .await;
                    let loaded_url = resource
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .map(|loaded| without_fragment(&loaded.url).to_owned());
                    if let Some(loaded_url) = loaded_url {
                        if !self.parent_brokered_script_urls.contains(&loaded_url)
                            && self.parent_brokered_script_urls.len()
                                >= MAX_CONTENT_BROKERED_SCRIPT_URLS
                        {
                            resource = Err(NativeEngineError::limit(
                                "parent-brokered worker script URLs",
                                MAX_CONTENT_BROKERED_SCRIPT_URLS,
                                self.parent_brokered_script_urls.len().saturating_add(1),
                            ));
                        } else {
                            self.parent_brokered_script_urls.insert(loaded_url);
                        }
                    }
                    let cookie_owner_url = response
                        .get("owner")
                        .and_then(|owner| owner.get("document_url"))
                        .and_then(Value::as_str)
                        .unwrap_or(document_url);
                    let mut broker_response =
                        parent_worker_script_response_payload(request_id, fetch_id, resource);
                    broker_response["document_cookie"] =
                        Value::String(loader.document_cookie(cookie_owner_url)?);
                    let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent worker script response",
                            NativeWorkerFailureKind::Protocol,
                            "worker script response could not be encoded",
                        )
                    })?;
                    write_frame(&mut self.stdin, &payload).await?;
                    continue;
                }
                if let Some(page_script_load) = response.get("page_script_load") {
                    let Some(fetch_id) = fetch_id else {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent page script broker",
                            NativeWorkerFailureKind::Protocol,
                            "page script request omitted its owner-bound request ID",
                        ));
                    };
                    if response.get("fetch_request").is_some()
                        || response.get("worker_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_media_load").is_some()
                        || response.get("page_font_load").is_some()
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent page script broker",
                            NativeWorkerFailureKind::Protocol,
                            "page script request also carried another load type",
                        ));
                    }
                    let owner = decode_content_cookie_owner(
                        response.get("owner").ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent page script broker",
                                NativeWorkerFailureKind::Protocol,
                                "page script request omitted its owner",
                            )
                        })?,
                        "content process parent page script owner",
                    )?;
                    if !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    ) {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent page script broker",
                            NativeWorkerFailureKind::Protocol,
                            "page script owner does not match the active document or captured load",
                        ));
                    }
                    if self.parent_brokered_script_owner.as_ref() != Some(&owner) {
                        self.parent_brokered_script_owner = Some(owner.clone());
                        self.parent_brokered_script_urls.clear();
                    }
                    let owner_document_url = without_fragment(&owner.document_url);
                    let requested_document_url = without_fragment(document_url);
                    if requested_document_url != owner_document_url
                        && !self
                            .parent_brokered_script_urls
                            .contains(requested_document_url)
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent page script broker",
                            NativeWorkerFailureKind::Protocol,
                            "page script request document is not owned by the active page",
                        ));
                    }
                    let referrer_url = match page_script_load.get("referrer_url") {
                        None | Some(Value::Null) => None,
                        Some(value) => Some(value.as_str().ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent page script broker",
                                NativeWorkerFailureKind::Protocol,
                                "page script request has an invalid referrer URL",
                            )
                        })?),
                    };
                    if let Some(referrer_url) = referrer_url {
                        validate_url_text("page script request referrer URL", referrer_url)?;
                        let referrer_url = without_fragment(referrer_url);
                        if referrer_url != owner_document_url
                            && !self.parent_brokered_script_urls.contains(referrer_url)
                        {
                            return Err(NativeEngineError::worker_failure(
                                "content process parent page script broker",
                                NativeWorkerFailureKind::Protocol,
                                "page script referrer is not owned by the active page",
                            ));
                        }
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    let mut resource =
                        load_parent_page_script_async(loader, document_url, href, page_script_load)
                            .await;
                    let loaded_url = resource
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .map(|loaded| without_fragment(&loaded.url).to_owned());
                    if let Some(loaded_url) = loaded_url {
                        if !self.parent_brokered_script_urls.contains(&loaded_url)
                            && self.parent_brokered_script_urls.len()
                                >= MAX_CONTENT_BROKERED_SCRIPT_URLS
                        {
                            resource = Err(NativeEngineError::limit(
                                "parent-brokered page script URLs",
                                MAX_CONTENT_BROKERED_SCRIPT_URLS,
                                self.parent_brokered_script_urls.len().saturating_add(1),
                            ));
                        } else {
                            self.parent_brokered_script_urls.insert(loaded_url);
                        }
                    }
                    let mut broker_response =
                        parent_page_script_response_payload(request_id, fetch_id, resource);
                    broker_response["document_cookie"] =
                        Value::String(loader.document_cookie(&owner.document_url)?);
                    let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent page script response",
                            NativeWorkerFailureKind::Protocol,
                            "page script response could not be encoded",
                        )
                    })?;
                    write_frame(&mut self.stdin, &payload).await?;
                    continue;
                }
                if let Some(page_stylesheet_load) = response.get("page_stylesheet_load") {
                    let Some(fetch_id) = fetch_id else {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent stylesheet broker",
                            NativeWorkerFailureKind::Protocol,
                            "stylesheet request omitted its owner-bound request ID",
                        ));
                    };
                    if response.get("fetch_request").is_some()
                        || response.get("worker_script_load").is_some()
                        || response.get("page_script_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_media_load").is_some()
                        || response.get("page_font_load").is_some()
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent stylesheet broker",
                            NativeWorkerFailureKind::Protocol,
                            "stylesheet request also carried another load type",
                        ));
                    }
                    let owner = decode_content_cookie_owner(
                        response.get("owner").ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent stylesheet broker",
                                NativeWorkerFailureKind::Protocol,
                                "stylesheet request omitted its owner",
                            )
                        })?,
                        "content process parent stylesheet owner",
                    )?;
                    if !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    ) || without_fragment(document_url) != without_fragment(&owner.document_url)
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent stylesheet broker",
                            NativeWorkerFailureKind::Protocol,
                            "stylesheet owner does not match the active document or captured load",
                        ));
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    let stylesheet = load_parent_page_stylesheet_async(
                        loader,
                        document_url,
                        href,
                        page_stylesheet_load,
                    )
                    .await;
                    let mut broker_response =
                        parent_stylesheet_response_payload(request_id, fetch_id, stylesheet);
                    broker_response["document_cookie"] =
                        Value::String(loader.document_cookie(&owner.document_url)?);
                    let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent stylesheet response",
                            NativeWorkerFailureKind::Protocol,
                            "stylesheet response could not be encoded",
                        )
                    })?;
                    write_frame(&mut self.stdin, &payload).await?;
                    continue;
                }
                if let Some(page_image_load) = response.get("page_image_load") {
                    let Some(fetch_id) = fetch_id else {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent image broker",
                            NativeWorkerFailureKind::Protocol,
                            "image request omitted its owner-bound request ID",
                        ));
                    };
                    if response.get("fetch_request").is_some()
                        || response.get("worker_script_load").is_some()
                        || response.get("page_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_media_load").is_some()
                        || response.get("page_font_load").is_some()
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent image broker",
                            NativeWorkerFailureKind::Protocol,
                            "image request also carried another load type",
                        ));
                    }
                    let owner = decode_content_cookie_owner(
                        response.get("owner").ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent image broker",
                                NativeWorkerFailureKind::Protocol,
                                "image request omitted its owner",
                            )
                        })?,
                        "content process parent image owner",
                    )?;
                    if !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    ) || without_fragment(document_url) != without_fragment(&owner.document_url)
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent image broker",
                            NativeWorkerFailureKind::Protocol,
                            "image owner does not match the active document or captured load",
                        ));
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    let image = if page_image_load
                        .get("initiator_type")
                        .and_then(Value::as_str)
                        == Some("video")
                    {
                        load_parent_page_video_poster_async(
                            loader,
                            document_url,
                            href,
                            page_image_load,
                        )
                        .await
                    } else {
                        load_parent_page_image_async(loader, document_url, href, page_image_load)
                            .await
                    };
                    let mut broker_response =
                        parent_image_response_payload(request_id, fetch_id, href, image);
                    broker_response["document_cookie"] =
                        Value::String(loader.document_cookie(&owner.document_url)?);
                    let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent image response",
                            NativeWorkerFailureKind::Protocol,
                            "image response could not be encoded",
                        )
                    })?;
                    write_frame(&mut self.stdin, &payload).await?;
                    continue;
                }
                if let Some(page_font_load) = response.get("page_font_load") {
                    let Some(fetch_id) = fetch_id else {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent font broker",
                            NativeWorkerFailureKind::Protocol,
                            "font request omitted its owner-bound request ID",
                        ));
                    };
                    if response.get("fetch_request").is_some()
                        || response.get("worker_script_load").is_some()
                        || response.get("page_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_media_load").is_some()
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent font broker",
                            NativeWorkerFailureKind::Protocol,
                            "font request also carried another load type",
                        ));
                    }
                    let owner = decode_content_cookie_owner(
                        response.get("owner").ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent font broker",
                                NativeWorkerFailureKind::Protocol,
                                "font request omitted its owner",
                            )
                        })?,
                        "content process parent font owner",
                    )?;
                    if !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    ) || without_fragment(document_url) != without_fragment(&owner.document_url)
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent font broker",
                            NativeWorkerFailureKind::Protocol,
                            "font owner does not match the active document or captured load",
                        ));
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    let font = load_parent_page_font_response_async(
                        loader,
                        document_url,
                        href,
                        page_font_load,
                    )
                    .await;
                    let mut broker_response =
                        parent_font_response_payload(request_id, fetch_id, font);
                    broker_response["document_cookie"] =
                        Value::String(loader.document_cookie(&owner.document_url)?);
                    let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent font response",
                            NativeWorkerFailureKind::Protocol,
                            "font response could not be encoded",
                        )
                    })?;
                    write_frame(&mut self.stdin, &payload).await?;
                    continue;
                }
                if let Some(page_media_load) = response.get("page_media_load") {
                    let Some(fetch_id) = fetch_id else {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent media broker",
                            NativeWorkerFailureKind::Protocol,
                            "media request omitted its owner-bound request ID",
                        ));
                    };
                    if response.get("fetch_request").is_some()
                        || response.get("worker_script_load").is_some()
                        || response.get("page_script_load").is_some()
                        || response.get("page_stylesheet_load").is_some()
                        || response.get("page_image_load").is_some()
                        || response.get("page_font_load").is_some()
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent media broker",
                            NativeWorkerFailureKind::Protocol,
                            "media request also carried another load type",
                        ));
                    }
                    let owner = decode_content_cookie_owner(
                        response.get("owner").ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent media broker",
                                NativeWorkerFailureKind::Protocol,
                                "media request omitted its owner",
                            )
                        })?,
                        "content process parent media owner",
                    )?;
                    let media_policy_document_url = response
                        .get("policy_document_url")
                        .and_then(Value::as_str)
                        .unwrap_or(document_url);
                    let media_base_url = response
                        .get("base_url")
                        .and_then(Value::as_str)
                        .unwrap_or(document_url);
                    if !parent_broker_owner_matches_operation(
                        self,
                        &request,
                        &owner,
                        parent_navigation_document_url.as_deref(),
                    ) || without_fragment(document_url) != without_fragment(&owner.document_url)
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent media broker",
                            NativeWorkerFailureKind::Protocol,
                            "media owner does not match the active document or captured load",
                        ));
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    let media = load_parent_page_media_async(
                        loader,
                        media_policy_document_url,
                        media_base_url,
                        href,
                        page_media_load,
                    )
                    .await;
                    let mut broker_response =
                        parent_media_response_payload(request_id, fetch_id, media);
                    broker_response["document_cookie"] =
                        Value::String(loader.document_cookie(&owner.document_url)?);
                    let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent media response",
                            NativeWorkerFailureKind::Protocol,
                            "media response could not be encoded",
                        )
                    })?;
                    write_frame(&mut self.stdin, &payload).await?;
                    continue;
                }
                let fetch = if let Some(fetch_request) = response.get("fetch_request") {
                    let request = decode_parent_fetch_request(
                        document_url,
                        href,
                        credentials,
                        fetch_request,
                    )?;
                    let top_level_navigation = fetch_request
                        .get("top_level_navigation")
                        .map(|value| {
                            value.as_bool().ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "content process parent Fetch broker",
                                    NativeWorkerFailureKind::Protocol,
                                    "top-level navigation marker must be boolean",
                                )
                            })
                        })
                        .transpose()?
                        .unwrap_or(false);
                    if top_level_navigation
                        && (!captured_load_fetch
                            || request.cors_mode != NativeCorsMode::Navigation
                            || request.method.as_str() != "GET"
                            || request.body.is_some())
                    {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "top-level navigation Fetch must be a captured, bodyless navigation GET",
                        ));
                    }
                    if !top_level_navigation && request.cors_mode == NativeCorsMode::Navigation {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "navigation-mode Fetch omitted its top-level navigation marker",
                        ));
                    }
                    if top_level_navigation {
                        let fetch_id = fetch_id.ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "content process parent Fetch cancellation",
                                NativeWorkerFailureKind::Protocol,
                                "top-level navigation Fetch omitted its cancellation ID",
                            )
                        })?;
                        if cancellable_parent_navigation_fetches.len() >= MAX_NATIVE_EFFECTS
                            || !cancellable_parent_navigation_fetches.insert(fetch_id)
                        {
                            return Err(NativeEngineError::worker_failure(
                                "content process parent Fetch cancellation",
                                NativeWorkerFailureKind::Protocol,
                                "top-level navigation Fetch cancellation ID is duplicated or exceeds its bound",
                            ));
                        }
                    }
                    for write in &cookie_writes {
                        loader.set_document_cookie(&write.owner.document_url, &write.value)?;
                    }
                    if top_level_navigation {
                        let fetch_id = fetch_id.expect("validated navigation preload ID");
                        let fetch =
                            loader.fetch_request_with_navigation_context_async(request, true);
                        tokio::pin!(fetch);
                        tokio::select! {
                            biased;
                            result = &mut fetch => result,
                            child_response = self.read_response_value() => {
                                let child_response = child_response?;
                                let cancelled_fetch_id = decode_parent_fetch_cancellation(
                                    &child_response,
                                    request_id,
                                    &mut cancellable_parent_navigation_fetches,
                                )?;
                                if cancelled_fetch_id != fetch_id {
                                    return Err(NativeEngineError::worker_failure(
                                        "content process parent Fetch cancellation",
                                        NativeWorkerFailureKind::Protocol,
                                        "cancellation targeted a different navigation Fetch",
                                    ));
                                }
                                self.acknowledge_parent_fetch_cancellation(request_id, fetch_id)
                                    .await?;
                                continue;
                            }
                        }
                    } else {
                        loader.fetch_request_with_headers_async(request).await
                    }
                } else {
                    if !cookie_writes.is_empty() {
                        return Err(NativeEngineError::worker_failure(
                            "content process parent Fetch broker",
                            NativeWorkerFailureKind::Protocol,
                            "cookie writes were returned without a script Fetch request",
                        ));
                    }
                    loader.fetch_async(document_url, href, credentials).await
                };
                let cookie_owner_url = response
                    .get("owner")
                    .and_then(|owner| owner.get("document_url"))
                    .and_then(Value::as_str)
                    .unwrap_or(document_url);
                let document_cookie = loader.document_cookie(cookie_owner_url)?;
                let csp_violations = loader.take_csp_violations();
                let mut broker_response = parent_fetch_response_payload(request_id, fetch);
                if let Some(fetch_id) = fetch_id {
                    broker_response["fetch_id"] = Value::from(fetch_id);
                }
                broker_response["document_cookie"] = Value::String(document_cookie);
                broker_response["csp_violations"] =
                    serde_json::to_value(csp_violations).map_err(|_| {
                        NativeEngineError::worker_failure(
                            "encode parent Fetch response",
                            NativeWorkerFailureKind::Protocol,
                            "Fetch CSP violations could not be encoded",
                        )
                    })?;
                let payload = serde_json::to_vec(&broker_response).map_err(|_| {
                    NativeEngineError::worker_failure(
                        "encode parent Fetch response",
                        NativeWorkerFailureKind::Protocol,
                        "parent Fetch response could not be encoded",
                    )
                })?;
                write_frame(&mut self.stdin, &payload).await?;
                continue;
            }
            if response.get("kind").and_then(Value::as_str) != Some("dialog_open") {
                if let Some(dialog) = resumed_dialog.take() {
                    dialog.finish();
                }
                return Ok(response);
            }
            if let Some(dialog) = resumed_dialog.take() {
                dialog.finish();
            }
            if response.get("request_id").and_then(Value::as_u64) != Some(request_id) {
                return Err(NativeEngineError::Worker {
                    operation: "route native dialog".into(),
                    reason: "native dialog belongs to a different content request".into(),
                });
            }
            let context_id = self
                .context_id
                .clone()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "route native dialog".into(),
                    reason: "content process has no bound browsing context".into(),
                })?;
            let frame_id = self
                .frame_id
                .clone()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "route native dialog".into(),
                    reason: "content process has no bound frame".into(),
                })?;
            let child_dialog_id = response
                .get("dialog_id")
                .and_then(Value::as_u64)
                .filter(|id| *id > 0)
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "route native dialog".into(),
                    reason: "content process dialog identifier is invalid".into(),
                })?;
            let dialog_url = response.get("url").and_then(Value::as_str).ok_or_else(|| {
                NativeEngineError::Worker {
                    operation: "route native dialog".into(),
                    reason: "content process dialog URL is missing".into(),
                }
            })?;
            let dialog = response
                .get("dialog")
                .cloned()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "route native dialog".into(),
                    reason: "content process dialog metadata is missing".into(),
                })
                .and_then(|dialog| {
                    serde_json::from_value::<NativeDialog>(dialog).map_err(|_| {
                        NativeEngineError::Worker {
                            operation: "route native dialog".into(),
                            reason: "content process dialog metadata is invalid".into(),
                        }
                    })
                })?;
            let dialog_control = self.dialog_control.clone();
            let dialog_wait = dialog_control.wait_for_resolution(
                &context_id,
                &frame_id,
                dialog_url,
                child_dialog_id,
                dialog,
            );
            tokio::pin!(dialog_wait);
            let resolution = tokio::select! {
                biased;
                status = self.child.wait() => {
                    let (kind, reason) = match status {
                        Ok(_) => (
                            NativeWorkerFailureKind::Exited,
                            "content process exited while a modal dialog was pending",
                        ),
                        Err(_) => (
                            NativeWorkerFailureKind::Transport,
                            "content process status could not be observed while a modal dialog was pending",
                        ),
                    };
                    self.mark_failed(kind);
                    return Err(NativeEngineError::worker_failure(
                        "content process modal dialog",
                        kind,
                        reason,
                    ));
                }
                resolution = &mut dialog_wait => resolution?,
            };
            let response = resolution.resolution.clone();
            let decision = serde_json::to_vec(&json!({
                "kind": "dialog_decision",
                "request_id": request_id,
                "dialog_id": child_dialog_id,
                "accepted": response.accepted,
                "prompt_value": response.prompt_value,
            }))
            .map_err(|_| NativeEngineError::Worker {
                operation: "encode native dialog decision".into(),
                reason: "native dialog decision could not be encoded".into(),
            })?;
            write_frame(&mut self.stdin, &decision).await?;
            resumed_dialog = Some(resolution);
        }
    }
}
