use super::*;

impl NativeContentProcess {
    pub(super) async fn handle_parent_websocket_request(
        &mut self,
        operation: &Value,
        response: &Value,
        parent_navigation_document_url: Option<&str>,
        parent_loader: Option<&mut NativeResourceLoader>,
    ) -> Result<Value, NativeEngineError> {
        let request_id = response.get("id").and_then(Value::as_u64).ok_or_else(|| {
            NativeEngineError::worker_failure(
                "parent WebSocket broker",
                NativeWorkerFailureKind::Protocol,
                "WebSocket request omitted its content-operation ID",
            )
        })?;
        if operation.get("id").and_then(Value::as_u64) != Some(request_id) {
            return Err(NativeEngineError::worker_failure(
                "parent WebSocket broker",
                NativeWorkerFailureKind::Protocol,
                "WebSocket request belongs to a different content operation",
            ));
        }
        let kind = response
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent WebSocket broker",
                    NativeWorkerFailureKind::Protocol,
                    "WebSocket request omitted its operation kind",
                )
            })?;
        let source_id = response
            .get("source_id")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value != 0)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent WebSocket broker",
                    NativeWorkerFailureKind::Protocol,
                    "WebSocket request has an invalid source ID",
                )
            })?;
        let worker_id = decode_optional_content_worker_id(response, "parent WebSocket worker ID")?;
        let owner = decode_content_cookie_owner(
            response.get("owner").ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent WebSocket broker",
                    NativeWorkerFailureKind::Protocol,
                    "WebSocket request omitted its parent-captured owner",
                )
            })?,
            "parent WebSocket owner",
        )?;
        let expected_owner = operation
            .get("owner")
            .map(|value| decode_content_cookie_owner(value, "parent script owner"))
            .transpose()?;
        if expected_owner
            .as_ref()
            .is_some_and(|expected_owner| expected_owner != &owner)
            || !parent_broker_owner_matches_operation(
                self,
                operation,
                &owner,
                parent_navigation_document_url,
            )
        {
            return Err(NativeEngineError::worker_failure(
                "parent WebSocket broker",
                NativeWorkerFailureKind::Protocol,
                "WebSocket request does not match the active document owner",
            ));
        }
        let loader = parent_loader.ok_or_else(|| {
            NativeEngineError::worker_failure(
                "parent WebSocket broker",
                NativeWorkerFailureKind::Protocol,
                "content process requested WebSocket service without a parent loader",
            )
        })?;
        let document_cookie = apply_parent_content_cookie_writes(
            loader,
            &owner,
            response,
            "parent WebSocket cookie-write journal",
        )?;
        let stream_id = response
            .get("stream_id")
            .and_then(Value::as_u64)
            .filter(|stream_id| *stream_id > 0);
        match kind {
            "parent_websocket_open_request" => {
                if stream_id.is_some() {
                    return Err(NativeEngineError::worker_failure(
                        "parent WebSocket open",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket open request carried a stream ID",
                    ));
                }
                let document_url = response
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "parent WebSocket open",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket open omitted its page-owner URL",
                        )
                    })?;
                let initiator_url = response
                    .get("initiator_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "parent WebSocket open",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket open omitted its initiator URL",
                        )
                    })?;
                let href = response
                    .get("href")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "parent WebSocket open",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket open omitted its URL",
                        )
                    })?;
                let protocols = response
                    .get("protocols")
                    .cloned()
                    .map(serde_json::from_value::<Vec<String>>)
                    .transpose()
                    .map_err(|_| {
                        NativeEngineError::worker_failure(
                            "parent WebSocket open",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket protocols are malformed",
                        )
                    })?
                    .unwrap_or_default();
                if without_fragment(document_url) != without_fragment(&owner.document_url) {
                    return Err(NativeEngineError::worker_failure(
                        "parent WebSocket open",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket page URL does not match its captured owner",
                    ));
                }
                if worker_id.is_none()
                    && without_fragment(initiator_url) != without_fragment(&owner.document_url)
                {
                    return Err(NativeEngineError::worker_failure(
                        "parent WebSocket open",
                        NativeWorkerFailureKind::Protocol,
                        "page WebSocket initiator does not match its owner",
                    ));
                }
                validate_websocket_protocols(&protocols)?;
                if self.parent_websocket_streams.len() >= MAX_NATIVE_WEBSOCKET_EVENTS {
                    return Ok(json!({
                        "kind": "parent_websocket_error",
                        "id": request_id,
                        "source_id": source_id,
                        "worker_id": worker_id,
                        "reason": "parent WebSocket stream limit reached",
                        "document_cookie": loader.document_cookie(&owner.document_url)?,
                        "csp_violations": [],
                    }));
                }
                let target = match loader.websocket_target(initiator_url, href) {
                    Ok(target) => target,
                    Err(error) => {
                        return Ok(json!({
                            "kind": "parent_websocket_error",
                            "id": request_id,
                            "source_id": source_id,
                            "worker_id": worker_id,
                            "reason": bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                            "document_cookie": loader.document_cookie(&owner.document_url)?,
                            "csp_violations": loader.take_csp_violations(),
                        }));
                    }
                };
                let origin_url = Url::parse(without_fragment(initiator_url)).map_err(|_| {
                    NativeEngineError::UnsupportedUrl {
                        reason: "WebSocket initiator URL is not valid HTTP(S) syntax".into(),
                    }
                })?;
                let origin = NativeOrigin::from_url(&origin_url)?;
                let request = match native_websocket_request(&target, &origin, &protocols) {
                    Ok(request) => request,
                    Err(error) => {
                        return Ok(json!({
                            "kind": "parent_websocket_error",
                            "id": request_id,
                            "source_id": source_id,
                            "worker_id": worker_id,
                            "reason": bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                            "document_cookie": loader.document_cookie(&owner.document_url)?,
                            "csp_violations": target.csp_violations,
                        }));
                    }
                };
                let target_url = target.url.clone();
                let csp_violations = target.csp_violations.clone();
                schedule_native_csp_report_deliveries(target.csp_report_deliveries);
                let connected =
                    timeout(NATIVE_WEBSOCKET_CONNECT_TIMEOUT, connect_async(request)).await;
                let (socket, handshake) = match connected {
                    Ok(Ok(connected)) => connected,
                    Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => {
                        let set_cookie_headers = response
                            .headers()
                            .get_all(tokio_tungstenite::tungstenite::http::header::SET_COOKIE)
                            .iter()
                            .filter_map(|value| value.to_str().ok().map(str::to_owned))
                            .collect::<Vec<_>>();
                        // Fetch processes cookies from an HTTP response before
                        // WebSocket rejects a non-101 handshake. Keep this
                        // response and the cookie values in the parent.
                        loader
                            .apply_websocket_response_cookies(&target_url, &set_cookie_headers)?;
                        return Ok(json!({
                            "kind": "parent_websocket_error",
                            "id": request_id,
                            "source_id": source_id,
                            "worker_id": worker_id,
                            "reason": bounded_websocket_text(
                                format!("HTTP error: {}", response.status()),
                                MAX_NATIVE_SCRIPT_BYTES,
                            ),
                            "document_cookie": loader.document_cookie(&owner.document_url)?,
                            "csp_violations": csp_violations,
                        }));
                    }
                    Ok(Err(error)) => {
                        return Ok(json!({
                            "kind": "parent_websocket_error",
                            "id": request_id,
                            "source_id": source_id,
                            "worker_id": worker_id,
                            "reason": bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                            "document_cookie": loader.document_cookie(&owner.document_url)?,
                            "csp_violations": csp_violations,
                        }));
                    }
                    Err(_) => {
                        return Ok(json!({
                            "kind": "parent_websocket_error",
                            "id": request_id,
                            "source_id": source_id,
                            "worker_id": worker_id,
                            "reason": "native WebSocket handshake timed out",
                            "document_cookie": loader.document_cookie(&owner.document_url)?,
                            "csp_violations": csp_violations,
                        }));
                    }
                };
                let selected_protocol = handshake
                    .headers()
                    .get(tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_owned();
                let set_cookie_headers = handshake
                    .headers()
                    .get_all(tokio_tungstenite::tungstenite::http::header::SET_COOKIE)
                    .iter()
                    .filter_map(|value| value.to_str().ok().map(str::to_owned))
                    .collect::<Vec<_>>();
                loader.apply_websocket_response_cookies(&target_url, &set_cookie_headers)?;
                let stream_id = self.next_parent_websocket_stream_id;
                self.next_parent_websocket_stream_id =
                    stream_id.checked_add(1).ok_or_else(|| {
                        NativeEngineError::limit(
                            "parent WebSocket stream IDs",
                            u64::MAX as usize,
                            usize::MAX,
                        )
                    })?;
                let origin = origin.serialized();
                self.parent_websocket_streams.insert(
                    stream_id,
                    NativeParentWebSocketStream {
                        owner: owner.clone(),
                        source_id,
                        worker_id,
                        origin: origin.clone(),
                        socket,
                    },
                );
                Ok(json!({
                    "kind": "parent_websocket_opened",
                    "id": request_id,
                    "source_id": source_id,
                    "worker_id": worker_id,
                    "stream_id": stream_id,
                    "origin": origin,
                    "protocol": selected_protocol,
                    "document_cookie": document_cookie.clone(),
                    "csp_violations": csp_violations,
                }))
            }
            "parent_websocket_read_request" => {
                let stream_id = stream_id.ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent WebSocket read",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket read has an invalid stream ID",
                    )
                })?;
                let outcome =
                    if let Some(stream) = self.parent_websocket_streams.get_mut(&stream_id) {
                        if stream.owner != owner
                            || stream.source_id != source_id
                            || stream.worker_id != worker_id
                        {
                            return Err(NativeEngineError::worker_failure(
                                "parent WebSocket read",
                                NativeWorkerFailureKind::Protocol,
                                "WebSocket stream does not belong to the requesting owner",
                            ));
                        }
                        match timeout(NATIVE_WEBSOCKET_POLL_INTERVAL, stream.socket.next()).await {
                            Err(_) => None,
                            Ok(Some(Ok(Message::Text(data)))) => {
                                let data = data.to_string();
                                if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                                    Some(Err("native WebSocket message exceeds its limit".into()))
                                } else {
                                    Some(Ok(NativeWebSocketEvent::MessageText {
                                        data,
                                        origin: stream.origin.clone(),
                                    }))
                                }
                            }
                            Ok(Some(Ok(Message::Binary(data)))) => {
                                if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                                    Some(Err("native WebSocket message exceeds its limit".into()))
                                } else {
                                    Some(Ok(NativeWebSocketEvent::MessageBinary {
                                        data: data.to_vec(),
                                        origin: stream.origin.clone(),
                                    }))
                                }
                            }
                            Ok(Some(Ok(Message::Close(frame)))) => {
                                let (code, reason) = frame
                                    .map(|frame| {
                                        (
                                            u16::from(frame.code),
                                            bounded_websocket_text(
                                                frame.reason,
                                                MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
                                            ),
                                        )
                                    })
                                    .unwrap_or((1005, String::new()));
                                Some(Ok(NativeWebSocketEvent::Close {
                                    code,
                                    reason,
                                    was_clean: true,
                                }))
                            }
                            Ok(Some(Ok(Message::Ping(data)))) => {
                                match stream.socket.send(Message::Pong(data)).await {
                                    Ok(()) => None,
                                    Err(error) => Some(Err(bounded_websocket_text(
                                        error.to_string(),
                                        MAX_NATIVE_SCRIPT_BYTES,
                                    ))),
                                }
                            }
                            Ok(Some(Ok(Message::Pong(_)))) => None,
                            Ok(Some(Ok(_))) => None,
                            Ok(Some(Err(error))) => Some(Err(bounded_websocket_text(
                                error.to_string(),
                                MAX_NATIVE_SCRIPT_BYTES,
                            ))),
                            Ok(None) => Some(Ok(NativeWebSocketEvent::Close {
                                code: 1006,
                                reason: String::new(),
                                was_clean: false,
                            })),
                        }
                    } else {
                        Some(Ok(NativeWebSocketEvent::Close {
                            code: 1006,
                            reason: String::new(),
                            was_clean: false,
                        }))
                    };
                let (kind, fields, remove_stream) = match outcome {
                    None => ("parent_websocket_pending", json!({}), false),
                    Some(Ok(NativeWebSocketEvent::MessageText { data, origin })) => (
                        "parent_websocket_text",
                        json!({"data": data, "origin": origin}),
                        false,
                    ),
                    Some(Ok(NativeWebSocketEvent::MessageBinary { data, origin })) => (
                        "parent_websocket_binary",
                        json!({
                            "data_base64": base64::engine::general_purpose::STANDARD.encode(data),
                            "origin": origin,
                        }),
                        false,
                    ),
                    Some(Ok(NativeWebSocketEvent::Close {
                        code,
                        reason,
                        was_clean,
                    })) => (
                        "parent_websocket_close",
                        json!({"code": code, "reason": reason, "was_clean": was_clean}),
                        true,
                    ),
                    Some(Ok(_)) => ("parent_websocket_pending", json!({}), false),
                    Some(Err(reason)) => (
                        "parent_websocket_error",
                        json!({"reason": reason, "csp_violations": []}),
                        true,
                    ),
                };
                if remove_stream {
                    self.parent_websocket_streams.remove(&stream_id);
                }
                let mut result = json!({
                    "kind": kind,
                    "id": request_id,
                    "source_id": source_id,
                    "worker_id": worker_id,
                    "stream_id": stream_id,
                    "document_cookie": loader.document_cookie(&owner.document_url)?,
                });
                if let (Some(result), Some(fields)) = (result.as_object_mut(), fields.as_object()) {
                    result.extend(fields.clone());
                }
                Ok(result)
            }
            "parent_websocket_send_request" => {
                let stream_id = stream_id.ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent WebSocket send",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket send has an invalid stream ID",
                    )
                })?;
                let message = response.get("message").ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent WebSocket send",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket send omitted its payload",
                    )
                })?;
                let message_type = message.get("message_type").and_then(Value::as_str);
                let outgoing = match message_type {
                    Some("text") => {
                        let data =
                            message.get("data").and_then(Value::as_str).ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "parent WebSocket send",
                                    NativeWorkerFailureKind::Protocol,
                                    "text WebSocket message omitted its data",
                                )
                            })?;
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "parent WebSocket text message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Text(data.to_owned().into())
                    }
                    Some("binary") => {
                        let encoded = message
                            .get("data_base64")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                NativeEngineError::worker_failure(
                                    "parent WebSocket send",
                                    NativeWorkerFailureKind::Protocol,
                                    "binary WebSocket message omitted its payload",
                                )
                            })?;
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .map_err(|_| {
                                NativeEngineError::worker_failure(
                                    "parent WebSocket send",
                                    NativeWorkerFailureKind::Protocol,
                                    "binary WebSocket message is not valid base64",
                                )
                            })?;
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "parent WebSocket binary message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Binary(data.into())
                    }
                    _ => {
                        return Err(NativeEngineError::worker_failure(
                            "parent WebSocket send",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket message type is unsupported",
                        ));
                    }
                };
                let result = if let Some(stream) = self.parent_websocket_streams.get_mut(&stream_id)
                {
                    if stream.owner != owner
                        || stream.source_id != source_id
                        || stream.worker_id != worker_id
                    {
                        return Err(NativeEngineError::worker_failure(
                            "parent WebSocket send",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket send does not match its stream owner",
                        ));
                    }
                    stream.socket.send(outgoing).await
                } else {
                    return Ok(json!({
                        "kind": "parent_websocket_error",
                        "id": request_id,
                        "source_id": source_id,
                        "worker_id": worker_id,
                        "stream_id": stream_id,
                        "reason": "parent WebSocket stream is no longer active",
                        "document_cookie": document_cookie.clone(),
                    }));
                };
                match result {
                    Ok(()) => Ok(json!({
                        "kind": "parent_websocket_sent",
                        "id": request_id,
                        "source_id": source_id,
                        "worker_id": worker_id,
                        "stream_id": stream_id,
                        "document_cookie": document_cookie.clone(),
                    })),
                    Err(error) => {
                        self.parent_websocket_streams.remove(&stream_id);
                        Ok(json!({
                            "kind": "parent_websocket_error",
                            "id": request_id,
                            "source_id": source_id,
                            "worker_id": worker_id,
                            "stream_id": stream_id,
                            "reason": bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                            "document_cookie": document_cookie.clone(),
                        }))
                    }
                }
            }
            "parent_websocket_close_request" => {
                let stream_id = stream_id.ok_or_else(|| {
                    NativeEngineError::worker_failure(
                        "parent WebSocket close",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket close has an invalid stream ID",
                    )
                })?;
                let code = response
                    .get("code")
                    .and_then(Value::as_u64)
                    .and_then(|code| u16::try_from(code).ok())
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "parent WebSocket close",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket close omitted a valid code",
                        )
                    })?;
                let reason = response
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "parent WebSocket close",
                            NativeWorkerFailureKind::Protocol,
                            "WebSocket close omitted its reason",
                        )
                    })?;
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
                let Some(mut stream) = self.parent_websocket_streams.remove(&stream_id) else {
                    return Ok(json!({
                        "kind": "parent_websocket_closed",
                        "id": request_id,
                        "source_id": source_id,
                        "worker_id": worker_id,
                        "stream_id": stream_id,
                        "was_clean": false,
                        "document_cookie": document_cookie.clone(),
                    }));
                };
                if stream.owner != owner
                    || stream.source_id != source_id
                    || stream.worker_id != worker_id
                {
                    self.parent_websocket_streams.insert(stream_id, stream);
                    return Err(NativeEngineError::worker_failure(
                        "parent WebSocket close",
                        NativeWorkerFailureKind::Protocol,
                        "WebSocket close does not match its stream owner",
                    ));
                }
                let close_frame = Message::Close(Some(
                    tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code:
                            tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(
                                code,
                            ),
                        reason: reason.to_owned().into(),
                    },
                ));
                let was_clean = stream.socket.send(close_frame).await.is_ok();
                let _ = stream.socket.close(None).await;
                Ok(json!({
                    "kind": "parent_websocket_closed",
                    "id": request_id,
                    "source_id": source_id,
                    "worker_id": worker_id,
                    "stream_id": stream_id,
                    "was_clean": was_clean,
                    "document_cookie": document_cookie.clone(),
                }))
            }
            _ => Err(NativeEngineError::worker_failure(
                "parent WebSocket broker",
                NativeWorkerFailureKind::Protocol,
                "content process requested an unsupported WebSocket operation",
            )),
        }
    }
}
