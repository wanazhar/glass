use super::*;

pub(super) async fn open_parent_websocket_connection(
    broker: &mut NativeContentFetchBroker<'_>,
    source_id: u32,
    worker_id: Option<u32>,
    initiator_url: &str,
    href: &str,
    protocols: &[String],
) -> Result<NativeWebSocketConnection, NativeEngineError> {
    let (event_sender, events) = mpsc::channel(MAX_NATIVE_WEBSOCKET_EVENTS);
    let parent_stream_id = match broker
        .open_websocket(source_id, worker_id, initiator_url, href, protocols)
        .await?
    {
        NativeParentWebSocketOpenResult::Opened(opened) => {
            queue_websocket_event(
                &event_sender,
                NativeWebSocketEvent::Open {
                    protocol: opened.protocol,
                    csp_violations: opened.csp_violations,
                },
            )
            .await;
            Some(opened.stream_id)
        }
        NativeParentWebSocketOpenResult::Failed {
            message,
            csp_violations,
        } => {
            queue_websocket_event(
                &event_sender,
                NativeWebSocketEvent::Error {
                    message,
                    csp_violations,
                },
            )
            .await;
            queue_websocket_event(
                &event_sender,
                NativeWebSocketEvent::Close {
                    code: 1006,
                    reason: String::new(),
                    was_clean: false,
                },
            )
            .await;
            None
        }
    };
    Ok(NativeWebSocketConnection {
        event_sender,
        events,
        parent_stream_id,
    })
}

pub(super) async fn process_websocket_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::WebSocketOpen {
                socket_id,
                href,
                protocols,
                worker_id: None,
            } => {
                if connections.contains_key(&socket_id) {
                    return Err(NativeEngineError::Network {
                        operation: "WebSocket open".into(),
                        reason: "WebSocket identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_WEBSOCKET_EVENTS {
                    return Err(NativeEngineError::limit(
                        "native WebSocket connections",
                        MAX_NATIVE_WEBSOCKET_EVENTS,
                        connections.len().saturating_add(1),
                    ));
                }
                let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::Worker {
                        operation: "WebSocket open".into(),
                        reason: "parent cookie and network authority is unavailable".into(),
                    }
                })?;
                let connection = open_parent_websocket_connection(
                    broker,
                    socket_id,
                    None,
                    document_url,
                    &href,
                    &protocols,
                )
                .await?;
                connections.insert(socket_id, connection);
            }
            NativeScriptCommand::WebSocketSend {
                socket_id,
                data,
                data_base64,
                worker_id: None,
            } => {
                let message = match (data, data_base64) {
                    (Some(data), None) => {
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Text(data.into())
                    }
                    (None, Some(data_base64)) => {
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(data_base64)
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "native WebSocket binary message",
                                    "must be valid base64",
                                )
                            })?;
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Binary(data.into())
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native WebSocket message",
                            "must contain exactly one text or binary payload",
                        ));
                    }
                };
                let stream_id = connections
                    .get(&socket_id)
                    .and_then(|connection| connection.parent_stream_id)
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "WebSocket send".into(),
                        reason: "WebSocket is not open".into(),
                    })?;
                let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::Worker {
                        operation: "WebSocket send".into(),
                        reason: "parent cookie and network authority is unavailable".into(),
                    }
                })?;
                match broker
                    .send_parent_websocket(socket_id, None, stream_id, message)
                    .await
                {
                    Ok(()) => {}
                    Err(error @ NativeEngineError::Network { .. }) => {
                        let connection = connections.get_mut(&socket_id).ok_or_else(|| {
                            NativeEngineError::Network {
                                operation: "WebSocket send".into(),
                                reason: "WebSocket identifier is not active".into(),
                            }
                        })?;
                        connection.parent_stream_id = None;
                        queue_websocket_event(
                            &connection.event_sender,
                            NativeWebSocketEvent::Error {
                                message: bounded_websocket_text(
                                    error.to_string(),
                                    MAX_NATIVE_SCRIPT_BYTES,
                                ),
                                csp_violations: Vec::new(),
                            },
                        )
                        .await;
                        queue_websocket_event(
                            &connection.event_sender,
                            NativeWebSocketEvent::Close {
                                code: 1006,
                                reason: String::new(),
                                was_clean: false,
                            },
                        )
                        .await;
                    }
                    Err(error) => return Err(error),
                }
            }
            NativeScriptCommand::WebSocketClose {
                socket_id,
                code,
                reason,
                worker_id: None,
            } => {
                if code != 1000 && !(3000..=4999).contains(&code) {
                    return Err(NativeEngineError::invalid(
                        "native WebSocket close code",
                        "must be 1000 or in the 3000-4999 range",
                    ));
                }
                if reason.len() > MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES {
                    return Err(NativeEngineError::limit(
                        "native WebSocket close reason",
                        MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
                        reason.len(),
                    ));
                }
                let stream_id = connections
                    .get(&socket_id)
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "WebSocket close".into(),
                        reason: "WebSocket identifier is not active".into(),
                    })?
                    .parent_stream_id;
                let was_clean = if let Some(stream_id) = stream_id {
                    let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                        NativeEngineError::Worker {
                            operation: "WebSocket close".into(),
                            reason: "parent cookie and network authority is unavailable".into(),
                        }
                    })?;
                    broker
                        .close_parent_websocket(socket_id, None, stream_id, code, &reason)
                        .await?
                } else {
                    false
                };
                let connection =
                    connections
                        .get_mut(&socket_id)
                        .ok_or_else(|| NativeEngineError::Network {
                            operation: "WebSocket close".into(),
                            reason: "WebSocket identifier is not active".into(),
                        })?;
                connection.parent_stream_id = None;
                queue_websocket_event(
                    &connection.event_sender,
                    NativeWebSocketEvent::Close {
                        code: if was_clean { code } else { 1006 },
                        reason: if was_clean { reason } else { String::new() },
                        was_clean,
                    },
                )
                .await;
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

pub(super) async fn process_worker_websocket_commands(
    commands: Vec<NativeWorkerWebSocketCommand>,
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    for request in commands {
        let NativeWorkerWebSocketCommand {
            worker_id,
            worker_url,
            command,
        } = request;
        match command {
            NativeScriptCommand::WebSocketOpen {
                socket_id,
                href,
                protocols,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket command",
                        "WebSocket command worker id does not match its owner",
                    ));
                }
                let key = (worker_id, socket_id);
                if connections.contains_key(&key) {
                    return Err(NativeEngineError::Network {
                        operation: "Worker WebSocket open".into(),
                        reason: "WebSocket identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_WEBSOCKET_EVENTS {
                    return Err(NativeEngineError::limit(
                        "native Worker WebSocket connections",
                        MAX_NATIVE_WEBSOCKET_EVENTS,
                        connections.len().saturating_add(1),
                    ));
                }
                let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::Worker {
                        operation: "Worker WebSocket open".into(),
                        reason: "parent cookie and network authority is unavailable".into(),
                    }
                })?;
                let connection = open_parent_websocket_connection(
                    broker,
                    socket_id,
                    Some(worker_id),
                    &worker_url,
                    &href,
                    &protocols,
                )
                .await?;
                connections.insert(key, connection);
            }
            NativeScriptCommand::WebSocketSend {
                socket_id,
                data,
                data_base64,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket command",
                        "WebSocket command worker id does not match its owner",
                    ));
                }
                let message = match (data, data_base64) {
                    (Some(data), None) => {
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native Worker WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Text(data.into())
                    }
                    (None, Some(data_base64)) => {
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(data_base64)
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "native Worker WebSocket binary message",
                                    "must be valid base64",
                                )
                            })?;
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native Worker WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Binary(data.into())
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native Worker WebSocket message",
                            "must contain exactly one text or binary payload",
                        ));
                    }
                };
                let stream_id = connections
                    .get(&(worker_id, socket_id))
                    .and_then(|connection| connection.parent_stream_id)
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "Worker WebSocket send".into(),
                        reason: "WebSocket is not open".into(),
                    })?;
                let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::Worker {
                        operation: "Worker WebSocket send".into(),
                        reason: "parent cookie and network authority is unavailable".into(),
                    }
                })?;
                match broker
                    .send_parent_websocket(socket_id, Some(worker_id), stream_id, message)
                    .await
                {
                    Ok(()) => {}
                    Err(error @ NativeEngineError::Network { .. }) => {
                        let connection =
                            connections
                                .get_mut(&(worker_id, socket_id))
                                .ok_or_else(|| NativeEngineError::Network {
                                    operation: "Worker WebSocket send".into(),
                                    reason: "WebSocket identifier is not active".into(),
                                })?;
                        connection.parent_stream_id = None;
                        queue_websocket_event(
                            &connection.event_sender,
                            NativeWebSocketEvent::Error {
                                message: bounded_websocket_text(
                                    error.to_string(),
                                    MAX_NATIVE_SCRIPT_BYTES,
                                ),
                                csp_violations: Vec::new(),
                            },
                        )
                        .await;
                        queue_websocket_event(
                            &connection.event_sender,
                            NativeWebSocketEvent::Close {
                                code: 1006,
                                reason: String::new(),
                                was_clean: false,
                            },
                        )
                        .await;
                    }
                    Err(error) => return Err(error),
                }
            }
            NativeScriptCommand::WebSocketClose {
                socket_id,
                code,
                reason,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket command",
                        "WebSocket command worker id does not match its owner",
                    ));
                }
                if code != 1000 && !(3000..=4999).contains(&code) {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket close code",
                        "must be 1000 or in the 3000-4999 range",
                    ));
                }
                if reason.len() > MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Worker WebSocket close reason",
                        MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
                        reason.len(),
                    ));
                }
                let stream_id = connections
                    .get(&(worker_id, socket_id))
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "Worker WebSocket close".into(),
                        reason: "WebSocket identifier is not active".into(),
                    })?
                    .parent_stream_id;
                let was_clean = if let Some(stream_id) = stream_id {
                    let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                        NativeEngineError::Worker {
                            operation: "Worker WebSocket close".into(),
                            reason: "parent cookie and network authority is unavailable".into(),
                        }
                    })?;
                    broker
                        .close_parent_websocket(
                            socket_id,
                            Some(worker_id),
                            stream_id,
                            code,
                            &reason,
                        )
                        .await?
                } else {
                    false
                };
                let connection = connections
                    .get_mut(&(worker_id, socket_id))
                    .ok_or_else(|| NativeEngineError::Network {
                        operation: "Worker WebSocket close".into(),
                        reason: "WebSocket identifier is not active".into(),
                    })?;
                connection.parent_stream_id = None;
                queue_websocket_event(
                    &connection.event_sender,
                    NativeWebSocketEvent::Close {
                        code: if was_clean { code } else { 1006 },
                        reason: if was_clean { reason } else { String::new() },
                        was_clean,
                    },
                )
                .await;
            }
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker WebSocket command",
                    "command is not a worker-owned WebSocket operation",
                ));
            }
        }
    }
    Ok(())
}

pub(super) async fn process_worker_event_source_commands(
    commands: Vec<NativeWorkerEventSourceCommand>,
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    for request in commands {
        let NativeWorkerEventSourceCommand {
            worker_id,
            worker_url,
            command,
        } = request;
        match command {
            NativeScriptCommand::EventSourceOpen {
                source_id,
                href,
                with_credentials,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker EventSource command",
                        "EventSource command worker id does not match its owner",
                    ));
                }
                let key = (worker_id, source_id);
                if connections.contains_key(&key) {
                    return Err(NativeEngineError::Network {
                        operation: "Worker EventSource open".into(),
                        reason: "EventSource identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_EVENTSOURCE_CONNECTIONS {
                    return Err(NativeEngineError::limit(
                        "native Worker EventSource connections",
                        MAX_NATIVE_EVENTSOURCE_CONNECTIONS,
                        connections.len().saturating_add(1),
                    ));
                }
                let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::Worker {
                        operation: "Worker EventSource open".into(),
                        reason: "parent cookie and network authority is unavailable".into(),
                    }
                })?;
                validate_url_text("native Worker EventSource initiator URL", &worker_url)?;
                validate_url_text("native Worker EventSource URL", &href)?;
                let initial_open = broker
                    .open_event_source(
                        source_id,
                        Some(worker_id),
                        &worker_url,
                        &href,
                        with_credentials,
                        "",
                    )
                    .await?;
                let (initial_open, parent_stream_id) = match initial_open {
                    NativeParentEventSourceOpenResult::Opened(open) => {
                        (Ok(open.clone()), Some(open.stream_id))
                    }
                    NativeParentEventSourceOpenResult::Failed {
                        message,
                        csp_violations,
                    } => (
                        Err(NativeParentEventSourceFailure {
                            message,
                            csp_violations,
                        }),
                        None,
                    ),
                };
                let mut connection = spawn_native_parent_event_source(
                    &worker_url,
                    &href,
                    with_credentials,
                    "",
                    initial_open,
                );
                connection.parent_stream_id = parent_stream_id;
                connections.insert(key, connection);
            }
            NativeScriptCommand::EventSourceClose {
                source_id,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker EventSource command",
                        "EventSource command worker id does not match its owner",
                    ));
                }
                let key = (worker_id, source_id);
                if let Some(stream_id) = connections
                    .get(&key)
                    .and_then(|connection| connection.parent_stream_id)
                {
                    let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                        NativeEngineError::Worker {
                            operation: "Worker EventSource close".into(),
                            reason: "parent cookie and network authority is unavailable".into(),
                        }
                    })?;
                    broker
                        .close_parent_event_source(source_id, Some(worker_id), stream_id)
                        .await?;
                }
                if let Some(connection) = connections.remove(&key) {
                    let _ = connection
                        .commands
                        .send(NativeEventSourceCommand::Close)
                        .await;
                }
            }
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker EventSource command",
                    "command is not a worker-owned EventSource operation",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn process_fetch_stream_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::FetchStreamRead {
                stream_id,
                worker_id: None,
            } => {
                let Some(connection) = connections.get_mut(&stream_id) else {
                    return Err(NativeEngineError::Network {
                        operation: "fetch response stream read".into(),
                        reason: "fetch response stream identifier is not active".into(),
                    });
                };
                connection.read_pending = true;
                connection
                    .commands
                    .try_send(NativeFetchStreamCommand::Read)
                    .map_err(|_| NativeEngineError::Network {
                        operation: "fetch response stream read".into(),
                        reason: "fetch response stream task is unavailable".into(),
                    })?;
            }
            NativeScriptCommand::FetchStreamCancel {
                stream_id,
                worker_id: None,
            } => {
                if let Some(connection) = connections.remove(&stream_id) {
                    let _ = connection
                        .commands
                        .try_send(NativeFetchStreamCommand::Cancel);
                }
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

pub(super) fn process_fetch_upload_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeFetchUploadConnection>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::FetchUploadChunk {
                stream_id,
                data_base64,
                worker_id: None,
            } => {
                let Some(connection) = connections.get_mut(&stream_id) else {
                    return Err(NativeEngineError::Network {
                        operation: "fetch request upload chunk".into(),
                        reason: "fetch request upload identifier is not active".into(),
                    });
                };
                if !connection.demand_pending {
                    return Err(NativeEngineError::Network {
                        operation: "fetch request upload chunk".into(),
                        reason: "fetch request upload chunk arrived without demand".into(),
                    });
                }
                let data = base64::engine::general_purpose::STANDARD
                    .decode(data_base64)
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "fetch request upload chunk",
                            "must be valid base64",
                        )
                    })?;
                let next_total = connection.total_bytes.saturating_add(data.len());
                let next_chunks = connection.chunk_count.saturating_add(1);
                if next_total > MAX_NATIVE_FORM_BODY_BYTES
                    || next_chunks > MAX_NATIVE_FETCH_UPLOAD_CHUNKS
                {
                    let connection = connections
                        .remove(&stream_id)
                        .expect("fetch upload connection was checked above");
                    let message = if next_total > MAX_NATIVE_FORM_BODY_BYTES {
                        "native fetch request upload body exceeds its limit"
                    } else {
                        "native fetch request upload chunk limit exceeded"
                    };
                    let _ = connection
                        .commands
                        .try_send(NativeFetchUploadCommand::Error {
                            message: message.into(),
                        });
                    continue;
                }
                let connection = connections
                    .get_mut(&stream_id)
                    .expect("fetch upload connection was checked above");
                connection.total_bytes = next_total;
                connection.chunk_count = next_chunks;
                connection.demand_pending = false;
                connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Chunk { data })
                    .map_err(|_| NativeEngineError::Network {
                        operation: "fetch request upload chunk".into(),
                        reason: "fetch request upload task is unavailable".into(),
                    })?;
            }
            NativeScriptCommand::FetchUploadEnd {
                stream_id,
                worker_id: None,
            } => {
                let Some(mut connection) = connections.remove(&stream_id) else {
                    return Err(NativeEngineError::Network {
                        operation: "fetch request upload end".into(),
                        reason: "fetch request upload identifier is not active".into(),
                    });
                };
                if !connection.demand_pending {
                    return Err(NativeEngineError::Network {
                        operation: "fetch request upload end".into(),
                        reason: "fetch request upload ended without demand".into(),
                    });
                }
                connection.demand_pending = false;
                connection
                    .commands
                    .try_send(NativeFetchUploadCommand::End)
                    .map_err(|_| NativeEngineError::Network {
                        operation: "fetch request upload end".into(),
                        reason: "fetch request upload task is unavailable".into(),
                    })?;
            }
            NativeScriptCommand::FetchUploadError {
                stream_id,
                message,
                worker_id: None,
            } => {
                let Some(connection) = connections.remove(&stream_id) else {
                    return Err(NativeEngineError::Network {
                        operation: "fetch request upload error".into(),
                        reason: "fetch request upload identifier is not active".into(),
                    });
                };
                let message = message
                    .chars()
                    .take(crate::limits::MAX_TEXT_BYTES)
                    .collect();
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Error { message });
            }
            NativeScriptCommand::FetchUploadCancel {
                stream_id,
                worker_id: None,
            } => {
                if let Some(connection) = connections.remove(&stream_id) {
                    let _ = connection
                        .commands
                        .try_send(NativeFetchUploadCommand::Cancel);
                }
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

pub(super) async fn process_event_source_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    _loader: Option<&NativeResourceLoader>,
    _document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::EventSourceOpen {
                source_id,
                href,
                with_credentials,
                worker_id: None,
            } => {
                if connections.contains_key(&source_id) {
                    return Err(NativeEngineError::Network {
                        operation: "EventSource open".into(),
                        reason: "EventSource identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_EVENTSOURCE_CONNECTIONS {
                    return Err(NativeEngineError::limit(
                        "native EventSource connections",
                        MAX_NATIVE_EVENTSOURCE_CONNECTIONS,
                        connections.len().saturating_add(1),
                    ));
                }
                validate_url_text("EventSource URL", &href)?;
                let broker = parent_fetch_broker.as_deref_mut().ok_or_else(|| {
                    NativeEngineError::Worker {
                        operation: "EventSource open".into(),
                        reason: "parent cookie and network authority is unavailable".into(),
                    }
                })?;
                let initiator_url = broker.owner.document_url.clone();
                let initial_open = broker
                    .open_event_source(source_id, None, &initiator_url, &href, with_credentials, "")
                    .await?;
                let (initial_open, parent_stream_id) = match initial_open {
                    NativeParentEventSourceOpenResult::Opened(open) => {
                        (Ok(open.clone()), Some(open.stream_id))
                    }
                    NativeParentEventSourceOpenResult::Failed {
                        message,
                        csp_violations,
                    } => (
                        Err(NativeParentEventSourceFailure {
                            message,
                            csp_violations,
                        }),
                        None,
                    ),
                };
                let mut connection = spawn_native_parent_event_source(
                    &initiator_url,
                    &href,
                    with_credentials,
                    "",
                    initial_open,
                );
                connection.parent_stream_id = parent_stream_id;
                connections.insert(source_id, connection);
            }
            NativeScriptCommand::EventSourceClose {
                source_id,
                worker_id: None,
            } => {
                if let Some(mut connection) = connections.remove(&source_id) {
                    let close_result = if let Some(stream_id) = connection.parent_stream_id.take() {
                        match parent_fetch_broker.as_deref_mut() {
                            Some(broker) => {
                                broker
                                    .close_parent_event_source(source_id, None, stream_id)
                                    .await
                            }
                            None => Err(NativeEngineError::Worker {
                                operation: "EventSource close".into(),
                                reason: "parent cookie and network authority is unavailable".into(),
                            }),
                        }
                    } else {
                        Ok(())
                    };
                    let _ = connection
                        .commands
                        .send(NativeEventSourceCommand::Close)
                        .await;
                    close_result?;
                }
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

pub(super) fn take_websocket_event(
    connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
) -> Option<(u32, NativeWebSocketEvent)> {
    let socket_ids = connections.keys().copied().collect::<Vec<_>>();
    for socket_id in socket_ids {
        let Some(connection) = connections.get_mut(&socket_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((socket_id, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&socket_id);
            }
        }
    }
    None
}

pub(super) fn take_worker_websocket_event(
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
) -> Option<((u32, u32), NativeWebSocketEvent)> {
    let socket_ids = connections.keys().copied().collect::<Vec<_>>();
    for key in socket_ids {
        let Some(connection) = connections.get_mut(&key) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((key, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&key);
            }
        }
    }
    None
}

pub(super) fn take_worker_event_source_event(
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
) -> Option<((u32, u32), NativeEventSourceEvent)> {
    let source_ids = connections.keys().copied().collect::<Vec<_>>();
    for key in source_ids {
        let Some(connection) = connections.get_mut(&key) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((key, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&key);
            }
        }
    }
    None
}

pub(super) async fn pump_parent_websocket_stream(
    connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    read_cursor: &mut u32,
) -> Result<bool, NativeEngineError> {
    let Some(broker) = parent_fetch_broker else {
        return Ok(false);
    };
    let socket_ids = connections.keys().copied().collect::<Vec<_>>();
    if socket_ids.is_empty() {
        return Ok(false);
    }
    let start = socket_ids
        .iter()
        .position(|socket_id| *socket_id >= *read_cursor)
        .unwrap_or(0);
    for offset in 0..socket_ids.len() {
        let socket_id = socket_ids[(start + offset) % socket_ids.len()];
        let Some(stream_id) = connections
            .get(&socket_id)
            .and_then(|connection| connection.parent_stream_id)
        else {
            continue;
        };
        *read_cursor = socket_id.wrapping_add(1).max(1);
        match broker
            .read_parent_websocket(socket_id, None, stream_id)
            .await?
        {
            NativeParentWebSocketRead::Pending => return Ok(false),
            NativeParentWebSocketRead::Event(event) => {
                let terminal = matches!(
                    &event,
                    NativeWebSocketEvent::Error { .. } | NativeWebSocketEvent::Close { .. }
                );
                let sent = if let Some(connection) = connections.get_mut(&socket_id) {
                    if terminal {
                        connection.parent_stream_id = None;
                    }
                    connection.event_sender.send(event).await.is_ok()
                } else {
                    false
                };
                if !sent {
                    connections.remove(&socket_id);
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(super) async fn pump_parent_worker_websocket_stream(
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    read_cursor: &mut Option<u64>,
) -> Result<bool, NativeEngineError> {
    let Some(broker) = parent_fetch_broker else {
        return Ok(false);
    };
    let keys = connections.keys().copied().collect::<Vec<_>>();
    if keys.is_empty() {
        return Ok(false);
    }
    let key_order =
        |(worker_id, socket_id): (u32, u32)| (u64::from(worker_id) << 32) | u64::from(socket_id);
    let start = keys
        .iter()
        .position(|key| Some(key_order(*key)) >= *read_cursor)
        .unwrap_or(0);
    for offset in 0..keys.len() {
        let (worker_id, socket_id) = keys[(start + offset) % keys.len()];
        let Some(stream_id) = connections
            .get(&(worker_id, socket_id))
            .and_then(|connection| connection.parent_stream_id)
        else {
            continue;
        };
        *read_cursor = Some(key_order((worker_id, socket_id)).wrapping_add(1));
        match broker
            .read_parent_websocket(socket_id, Some(worker_id), stream_id)
            .await?
        {
            NativeParentWebSocketRead::Pending => return Ok(false),
            NativeParentWebSocketRead::Event(event) => {
                let terminal = matches!(
                    &event,
                    NativeWebSocketEvent::Error { .. } | NativeWebSocketEvent::Close { .. }
                );
                let sent = if let Some(connection) = connections.get_mut(&(worker_id, socket_id)) {
                    if terminal {
                        connection.parent_stream_id = None;
                    }
                    connection.event_sender.send(event).await.is_ok()
                } else {
                    false
                };
                if !sent {
                    connections.remove(&(worker_id, socket_id));
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(super) async fn pump_worker_websocket_event(
    workers: &mut NativeWorkerRegistry,
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
    loader: &mut NativeResourceLoader,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    read_cursor: &mut Option<u64>,
) -> Result<bool, NativeEngineError> {
    let mut pending_event = take_worker_websocket_event(connections);
    if pending_event.is_none()
        && pump_parent_worker_websocket_stream(
            connections,
            parent_fetch_broker.as_deref_mut(),
            read_cursor,
        )
        .await?
    {
        pending_event = take_worker_websocket_event(connections);
    }
    let Some(((worker_id, socket_id), event)) = pending_event else {
        return Ok(false);
    };
    let remove_after_dispatch = matches!(&event, NativeWebSocketEvent::Close { .. });
    workers
        .dispatch_websocket_event(
            worker_id,
            socket_id,
            &websocket_event_payload(&event),
            websocket_event_csp_violations(&event),
            loader,
        )
        .await?;
    process_worker_websocket_commands(
        workers.take_websocket_commands(),
        connections,
        parent_fetch_broker,
    )
    .await?;
    if remove_after_dispatch {
        connections.remove(&(worker_id, socket_id));
    }
    Ok(true)
}

pub(super) async fn pump_worker_event_source_event(
    workers: &mut NativeWorkerRegistry,
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
    loader: &mut NativeResourceLoader,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<bool, NativeEngineError> {
    let Some(((worker_id, source_id), event)) = take_worker_event_source_event(connections) else {
        return Ok(false);
    };
    if let NativeEventSourceEvent::Reconnect { last_event_id } = &event {
        let (initiator_url, href, with_credentials, old_stream_id) = connections
            .get_mut(&(worker_id, source_id))
            .map(|connection| {
                (
                    connection.parent_initiator_url.clone(),
                    connection.parent_href.clone(),
                    connection.parent_with_credentials,
                    connection.parent_stream_id.take(),
                )
            })
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent Worker EventSource reconnect",
                    NativeWorkerFailureKind::Protocol,
                    "reconnecting Worker EventSource is no longer active",
                )
            })?;
        let broker =
            parent_fetch_broker
                .as_deref_mut()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "parent Worker EventSource reconnect".into(),
                    reason: "parent cookie and network authority is unavailable".into(),
                })?;
        if let Some(stream_id) = old_stream_id {
            broker
                .close_parent_event_source(source_id, Some(worker_id), stream_id)
                .await?;
        }
        let reopened = broker
            .open_event_source(
                source_id,
                Some(worker_id),
                &initiator_url,
                &href,
                with_credentials,
                last_event_id,
            )
            .await?;
        let connection = connections
            .get_mut(&(worker_id, source_id))
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent Worker EventSource reconnect",
                    NativeWorkerFailureKind::Protocol,
                    "reconnecting Worker EventSource closed while its request was opening",
                )
            })?;
        let command = match reopened {
            NativeParentEventSourceOpenResult::Opened(open) => {
                connection.parent_stream_id = Some(open.stream_id);
                NativeEventSourceCommand::Reopened {
                    origin: open.origin,
                    csp_violations: open.csp_violations,
                }
            }
            NativeParentEventSourceOpenResult::Failed {
                message,
                csp_violations,
            } => NativeEventSourceCommand::OpenFailed {
                message,
                csp_violations,
            },
        };
        connection.commands.send(command).await.map_err(|_| {
            NativeEngineError::worker_failure(
                "parent Worker EventSource reconnect",
                NativeWorkerFailureKind::Transport,
                "EventSource parser task terminated before receiving its reconnect result",
            )
        })?;
        return Ok(true);
    }
    if let NativeEventSourceEvent::Open { cookie_changes, .. } = &event
        && !cookie_changes.is_empty()
    {
        return Err(NativeEngineError::worker_failure(
            "parent Worker EventSource cookies",
            NativeWorkerFailureKind::Protocol,
            "parent-owned Worker EventSource returned child cookie changes",
        ));
    }
    if matches!(&event, NativeEventSourceEvent::Close)
        && let Some(stream_id) = connections
            .get_mut(&(worker_id, source_id))
            .and_then(|connection| connection.parent_stream_id.take())
    {
        let broker =
            parent_fetch_broker
                .as_deref_mut()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "parent Worker EventSource close".into(),
                    reason: "parent cookie and network authority is unavailable".into(),
                })?;
        broker
            .close_parent_event_source(source_id, Some(worker_id), stream_id)
            .await?;
    }
    let remove_after_dispatch = matches!(&event, NativeEventSourceEvent::Close);
    workers
        .dispatch_event_source_event(
            worker_id,
            source_id,
            &event_source_event_payload(&event),
            event_source_csp_violations(&event),
            loader,
        )
        .await?;
    process_worker_event_source_commands(
        workers.take_event_source_commands(),
        connections,
        parent_fetch_broker,
    )
    .await?;
    if remove_after_dispatch {
        connections.remove(&(worker_id, source_id));
    }
    Ok(true)
}

pub(super) fn take_event_source_event(
    connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
) -> Option<(u32, NativeEventSourceEvent)> {
    let source_ids = connections.keys().copied().collect::<Vec<_>>();
    for source_id in source_ids {
        let Some(connection) = connections.get_mut(&source_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((source_id, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&source_id);
            }
        }
    }
    None
}

pub(super) async fn pump_parent_event_source_stream(
    connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    cursor: &mut u32,
) -> Result<bool, NativeEngineError> {
    let source_ids = connections
        .iter()
        .filter_map(|(source_id, connection)| connection.parent_stream_id.map(|_| *source_id))
        .collect::<Vec<_>>();
    let Some(source_id) = source_ids
        .iter()
        .copied()
        .find(|source_id| *source_id > *cursor)
        .or_else(|| source_ids.first().copied())
    else {
        return Ok(false);
    };
    let stream_id = connections
        .get(&source_id)
        .and_then(|connection| connection.parent_stream_id)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "parent EventSource stream",
                NativeWorkerFailureKind::Protocol,
                "selected EventSource stream disappeared before it could be read",
            )
        })?;
    *cursor = source_id;
    let broker = parent_fetch_broker.ok_or_else(|| NativeEngineError::Worker {
        operation: "parent EventSource stream".into(),
        reason: "parent cookie and network authority is unavailable".into(),
    })?;
    let read = broker
        .read_parent_event_source(source_id, None, stream_id)
        .await;
    let Some(connection) = connections.get_mut(&source_id) else {
        return Ok(false);
    };
    match read {
        Ok(NativeParentEventSourceRead::Chunk(chunk)) => {
            connection
                .commands
                .send(NativeEventSourceCommand::Chunk(chunk))
                .await
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent EventSource stream",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving a response chunk",
                    )
                })?;
            Ok(true)
        }
        Ok(NativeParentEventSourceRead::Pending) => Ok(false),
        Ok(NativeParentEventSourceRead::End) => {
            connection.parent_stream_id = None;
            connection
                .commands
                .send(NativeEventSourceCommand::End)
                .await
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent EventSource stream",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving end of stream",
                    )
                })?;
            Ok(true)
        }
        Err(NativeEngineError::Network { reason, .. }) => {
            connection.parent_stream_id = None;
            connection
                .commands
                .send(NativeEventSourceCommand::Error(reason))
                .await
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent EventSource stream",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving its stream error",
                    )
                })?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn pump_parent_worker_event_source_stream(
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    cursor: &mut Option<(u32, u32)>,
) -> Result<bool, NativeEngineError> {
    let keys = connections
        .iter()
        .filter_map(|(key, connection)| connection.parent_stream_id.map(|_| *key))
        .collect::<Vec<_>>();
    let Some((worker_id, source_id)) = keys
        .iter()
        .copied()
        .find(|key| cursor.is_none_or(|cursor| *key > cursor))
        .or_else(|| keys.first().copied())
    else {
        return Ok(false);
    };
    let stream_id = connections
        .get(&(worker_id, source_id))
        .and_then(|connection| connection.parent_stream_id)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "parent Worker EventSource stream",
                NativeWorkerFailureKind::Protocol,
                "selected Worker EventSource stream disappeared before it could be read",
            )
        })?;
    *cursor = Some((worker_id, source_id));
    let broker = parent_fetch_broker.ok_or_else(|| NativeEngineError::Worker {
        operation: "parent Worker EventSource stream".into(),
        reason: "parent cookie and network authority is unavailable".into(),
    })?;
    let read = broker
        .read_parent_event_source(source_id, Some(worker_id), stream_id)
        .await;
    let Some(connection) = connections.get_mut(&(worker_id, source_id)) else {
        return Ok(false);
    };
    match read {
        Ok(NativeParentEventSourceRead::Chunk(chunk)) => {
            connection
                .commands
                .send(NativeEventSourceCommand::Chunk(chunk))
                .await
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent Worker EventSource stream",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving a response chunk",
                    )
                })?;
            Ok(true)
        }
        Ok(NativeParentEventSourceRead::Pending) => Ok(false),
        Ok(NativeParentEventSourceRead::End) => {
            connection.parent_stream_id = None;
            connection
                .commands
                .send(NativeEventSourceCommand::End)
                .await
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent Worker EventSource stream",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving end of stream",
                    )
                })?;
            Ok(true)
        }
        Err(NativeEngineError::Network { reason, .. }) => {
            connection.parent_stream_id = None;
            connection
                .commands
                .send(NativeEventSourceCommand::Error(reason))
                .await
                .map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent Worker EventSource stream",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving its stream error",
                    )
                })?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn take_fetch_stream_event(
    connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
) -> Option<(u32, NativeFetchStreamEvent)> {
    let stream_ids = connections.keys().copied().collect::<Vec<_>>();
    for stream_id in stream_ids {
        let Some(connection) = connections.get_mut(&stream_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => {
                connection.read_pending = false;
                return Some((stream_id, event));
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&stream_id);
            }
        }
    }
    None
}

pub(super) fn take_fetch_upload_event(
    connections: &mut BTreeMap<u32, NativeFetchUploadConnection>,
) -> Option<u32> {
    let stream_ids = connections.keys().copied().collect::<Vec<_>>();
    for stream_id in stream_ids {
        let Some(connection) = connections.get_mut(&stream_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(NativeFetchUploadEvent::Demand) => {
                connection.demand_pending = true;
                return Some(stream_id);
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&stream_id);
            }
        }
    }
    None
}

pub fn websocket_event_payload(event: &NativeWebSocketEvent) -> Value {
    match event {
        NativeWebSocketEvent::Open { protocol, .. } => {
            json!({"type": "open", "protocol": protocol})
        }
        NativeWebSocketEvent::MessageText { data, origin } => {
            json!({"type": "message", "data": data, "binary": false, "origin": origin})
        }
        NativeWebSocketEvent::MessageBinary { data, origin } => json!({
            "type": "message",
            "data": "",
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(data),
            "binary": true,
            "origin": origin,
        }),
        NativeWebSocketEvent::Error { message, .. } => {
            json!({"type": "error", "message": message})
        }
        NativeWebSocketEvent::Close {
            code,
            reason,
            was_clean,
        } => json!({
            "type": "close",
            "code": code,
            "reason": reason,
            "wasClean": was_clean,
        }),
    }
}

pub fn websocket_event_csp_violations(event: &NativeWebSocketEvent) -> &[NativeCspViolation] {
    match event {
        NativeWebSocketEvent::Open { csp_violations, .. }
        | NativeWebSocketEvent::Error { csp_violations, .. } => csp_violations,
        NativeWebSocketEvent::MessageText { .. }
        | NativeWebSocketEvent::MessageBinary { .. }
        | NativeWebSocketEvent::Close { .. } => &[],
    }
}

pub(super) fn event_source_event_payload(event: &NativeEventSourceEvent) -> Value {
    match event {
        NativeEventSourceEvent::Open { origin, .. } => {
            json!({"type": "open", "origin": origin})
        }
        NativeEventSourceEvent::Message {
            event,
            data,
            last_event_id,
            origin,
        } => json!({
            "type": "message",
            "event": event,
            "data": data,
            "lastEventId": last_event_id,
            "origin": origin,
        }),
        NativeEventSourceEvent::Error { message, .. } => {
            json!({"type": "error", "message": message})
        }
        NativeEventSourceEvent::Reconnect { .. } => {
            json!({"type": "error", "message": "EventSource reconnecting"})
        }
        NativeEventSourceEvent::Close => json!({"type": "close"}),
    }
}

pub(super) fn event_source_csp_violations(event: &NativeEventSourceEvent) -> &[NativeCspViolation] {
    match event {
        NativeEventSourceEvent::Open { csp_violations, .. }
        | NativeEventSourceEvent::Error { csp_violations, .. } => csp_violations,
        NativeEventSourceEvent::Message { .. }
        | NativeEventSourceEvent::Reconnect { .. }
        | NativeEventSourceEvent::Close => &[],
    }
}

pub(super) fn fetch_stream_event_payload(event: &NativeFetchStreamEvent) -> Value {
    match event {
        NativeFetchStreamEvent::Chunk { data } => json!({
            "type": "chunk",
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(data),
        }),
        NativeFetchStreamEvent::End => json!({"type": "end"}),
        NativeFetchStreamEvent::Error { message } => {
            json!({"type": "error", "message": message})
        }
    }
}

pub(super) fn fetch_upload_event_payload() -> Value {
    json!({"type": "demand"})
}
