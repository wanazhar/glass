use super::*;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};

pub(in crate::native_engine::content_process) fn encode_document_snapshot_base64(
    bytes: &[u8],
) -> String {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    if encoder.write_all(bytes).is_ok()
        && let Ok(compressed) = encoder.finish()
        && compressed.len() < bytes.len()
    {
        return base64::engine::general_purpose::STANDARD.encode(compressed);
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub(in crate::native_engine::content_process) fn decode_document_snapshot_base64(
    encoded: &str,
    operation: &str,
) -> Result<Vec<u8>, NativeEngineError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid document snapshot".into(),
        })?;
    let is_zlib = bytes.get(..2).is_some_and(|header| {
        header[0] == 0x78 && u16::from_be_bytes([header[0], header[1]]) % 31 == 0
    });
    if !is_zlib {
        if bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
            return Err(NativeEngineError::limit(
                "content-process document snapshot",
                MAX_CONTENT_DOCUMENT_WIRE_BYTES,
                bytes.len(),
            ));
        }
        return Ok(bytes);
    }

    let mut decoder = ZlibDecoder::new(bytes.as_slice());
    let mut decoded = Vec::new();
    decoder
        .by_ref()
        .take((MAX_CONTENT_DOCUMENT_WIRE_BYTES as u64).saturating_add(1))
        .read_to_end(&mut decoded)
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned a malformed compressed document snapshot".into(),
        })?;
    if decoded.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            decoded.len(),
        ));
    }
    Ok(decoded)
}

pub(in crate::native_engine::content_process) fn parent_broker_owner_matches_operation(
    process: &NativeContentProcess,
    request: &Value,
    owner: &NativeContentCookieOwner,
    parent_navigation_document_url: Option<&str>,
) -> bool {
    let owner_url = without_fragment(&owner.document_url);
    let is_current_document = process.context_id.as_deref() == Some(owner.context_id.as_str())
        && process.frame_id.as_deref() == Some(owner.frame_id.as_str())
        && process.current_document_generation == Some(owner.generation)
        && process
            .current_document_url
            .as_deref()
            .is_some_and(|current| without_fragment(current) == owner_url);
    if is_current_document {
        return true;
    }

    let captured_generation = request
        .get("generation")
        .and_then(Value::as_u64)
        .and_then(|generation| u32::try_from(generation).ok());
    let captured_url = request.get("url").and_then(Value::as_str);
    request.get("kind").and_then(Value::as_str) == Some("load")
        && process.context_id.as_deref() == Some(owner.context_id.as_str())
        && process.frame_id.as_deref() == Some(owner.frame_id.as_str())
        && captured_generation == Some(owner.generation)
        && (captured_url.is_some_and(|url| without_fragment(url) == owner_url)
            || parent_navigation_document_url.is_some_and(|url| without_fragment(url) == owner_url))
}

pub(in crate::native_engine::content_process) fn decode_parent_fetch_cancellation(
    response: &Value,
    request_id: u64,
    cancellable_fetches: &mut BTreeSet<u32>,
) -> Result<u32, NativeEngineError> {
    let fetch_id = response
        .get("fetch_id")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "content process parent Fetch cancellation",
                NativeWorkerFailureKind::Protocol,
                "cancellation request has an invalid Fetch ID",
            )
        })?;
    if response.get("kind").and_then(Value::as_str) != Some("parent_fetch_cancel")
        || response.get("id").and_then(Value::as_u64) != Some(request_id)
        || !cancellable_fetches.remove(&fetch_id)
    {
        return Err(NativeEngineError::worker_failure(
            "content process parent Fetch cancellation",
            NativeWorkerFailureKind::Protocol,
            "cancellation request does not match a parent-brokered navigation Fetch",
        ));
    }
    Ok(fetch_id)
}

pub(in crate::native_engine::content_process) fn decode_optional_content_worker_id(
    request: &Value,
    operation: &str,
) -> Result<Option<u32>, NativeEngineError> {
    match request.get("worker_id") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|worker_id| *worker_id > 0)
            .map(Some)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    operation,
                    NativeWorkerFailureKind::Protocol,
                    "worker EventSource ID must be a positive integer",
                )
            }),
    }
}

pub(in crate::native_engine::content_process) fn apply_parent_content_cookie_writes(
    loader: &mut NativeResourceLoader,
    owner: &NativeContentCookieOwner,
    response: &Value,
    operation: &'static str,
) -> Result<String, NativeEngineError> {
    let writes = serde_json::from_value::<Vec<NativeContentCookieWrite>>(
        response.get("cookie_writes").cloned().ok_or_else(|| {
            NativeEngineError::worker_failure(
                operation,
                NativeWorkerFailureKind::Protocol,
                "parent-brokered request omitted its cookie-write journal",
            )
        })?,
    )
    .map_err(|_| {
        NativeEngineError::worker_failure(
            operation,
            NativeWorkerFailureKind::Protocol,
            "parent-brokered cookie-write journal is malformed",
        )
    })?;
    if writes.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "parent-owned document.cookie writes",
            MAX_NATIVE_EFFECTS,
            writes.len(),
        ));
    }
    let mut bytes = 0usize;
    for write in &writes {
        if write.owner != *owner || write.value.len() > crate::limits::MAX_TEXT_BYTES {
            return Err(NativeEngineError::worker_failure(
                operation,
                NativeWorkerFailureKind::Protocol,
                "cookie write is oversized or belongs to another document owner",
            ));
        }
        bytes = bytes.saturating_add(write.value.len());
    }
    if bytes > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
        return Err(NativeEngineError::limit(
            "parent-owned document.cookie write journal",
            MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
            bytes,
        ));
    }
    for write in writes {
        loader.set_document_cookie(&owner.document_url, &write.value)?;
    }
    loader.document_cookie(&owner.document_url)
}

pub(in crate::native_engine::content_process) async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    payload: &[u8],
) -> Result<(), NativeEngineError> {
    if payload.len() > MAX_CONTENT_IPC_FRAME_BYTES {
        if std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").is_some() {
            let field_sizes = serde_json::from_slice::<Value>(payload)
                .ok()
                .and_then(|value| value.as_object().cloned())
                .map(|fields| {
                    let mut sizes = fields
                        .iter()
                        .map(|(name, value)| {
                            let size = serde_json::to_vec(value).map_or(0, |bytes| bytes.len());
                            (name.clone(), size)
                        })
                        .collect::<Vec<_>>();
                    sizes.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
                    sizes.truncate(12);
                    sizes
                });
            eprintln!(
                "native content worker oversized IPC frame: {} bytes; largest fields: {field_sizes:?}",
                payload.len()
            );
        }
        return Err(NativeEngineError::limit(
            "content IPC frame",
            MAX_CONTENT_IPC_FRAME_BYTES,
            payload.len(),
        ));
    }
    let length = u32::try_from(payload.len()).map_err(|_| {
        NativeEngineError::limit("content IPC frame", u32::MAX as usize, payload.len())
    })?;
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "write content IPC".into(),
            reason: "content process pipe is unavailable".into(),
        })?;
    writer
        .write_all(payload)
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "write content IPC".into(),
            reason: "content process pipe is unavailable".into(),
        })?;
    writer.flush().await.map_err(|_| NativeEngineError::Worker {
        operation: "write content IPC".into(),
        reason: "content process pipe is unavailable".into(),
    })
}

pub(in crate::native_engine::content_process) async fn read_frame(
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<Vec<u8>, NativeEngineError> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "read content IPC".into(),
            reason: "content process exited or closed its pipe".into(),
        })?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_CONTENT_IPC_FRAME_BYTES {
        return Err(NativeEngineError::limit(
            "content IPC frame",
            MAX_CONTENT_IPC_FRAME_BYTES,
            length,
        ));
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "read content IPC".into(),
            reason: "content process returned a truncated frame".into(),
        })?;
    Ok(payload)
}

pub(in crate::native_engine::content_process) async fn forward_content_process_output(
    mut reader: impl AsyncRead + Unpin,
    sender: mpsc::Sender<Result<Vec<u8>, NativeEngineError>>,
    notify: Option<Arc<tokio::sync::Notify>>,
) {
    loop {
        let frame = read_frame(&mut reader).await;
        let stop = frame.is_err();
        if sender.send(frame).await.is_err() {
            break;
        }
        if let Some(notify) = notify.as_ref() {
            notify.notify_one();
        }
        if stop {
            break;
        }
    }
}

pub(in crate::native_engine::content_process) fn discard_cancelled_parent_fetch_frame(
    value: &Value,
    cancelled_parent_fetches: &mut BTreeSet<(u64, u32)>,
) -> Result<bool, NativeEngineError> {
    if value.get("kind").and_then(Value::as_str) == Some("parent_fetch_cancelled") {
        let request_id = value.get("id").and_then(Value::as_u64).ok_or_else(|| {
            NativeEngineError::worker_failure(
                "parent Fetch cancellation acknowledgement",
                NativeWorkerFailureKind::Protocol,
                "parent omitted the content request ID",
            )
        })?;
        let fetch_id = value
            .get("fetch_id")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value != 0)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "parent Fetch cancellation acknowledgement",
                    NativeWorkerFailureKind::Protocol,
                    "parent returned an invalid Fetch request ID",
                )
            })?;
        cancelled_parent_fetches.remove(&(request_id, fetch_id));
        return Ok(true);
    }
    let Some(request_id) = value.get("id").and_then(Value::as_u64) else {
        return Ok(false);
    };
    let Some(fetch_id) = value
        .get("fetch_id")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0)
    else {
        return Ok(false);
    };
    Ok(cancelled_parent_fetches.remove(&(request_id, fetch_id)))
}

pub(in crate::native_engine::content_process) async fn receive_content_ipc_response(
    ipc_requests: &mut mpsc::Receiver<Result<Vec<u8>, NativeEngineError>>,
    cancelled_parent_fetches: &mut BTreeSet<(u64, u32)>,
    operation: &'static str,
) -> Result<Vec<u8>, NativeEngineError> {
    loop {
        let payload = ipc_requests
            .recv()
            .await
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "parent closed the broker response channel".into(),
            })??;
        let value: Value = serde_json::from_slice(&payload).map_err(|_| {
            NativeEngineError::worker_failure(
                operation,
                NativeWorkerFailureKind::Protocol,
                "parent returned invalid broker JSON",
            )
        })?;
        if discard_cancelled_parent_fetch_frame(&value, cancelled_parent_fetches)? {
            continue;
        }
        return Ok(payload);
    }
}

pub(in crate::native_engine::content_process) fn write_sync_frame(
    mut writer: impl Write,
    payload: &[u8],
) -> std::io::Result<()> {
    if payload.len() > MAX_CONTENT_IPC_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "content IPC frame exceeds its byte limit",
        ));
    }
    let length = u32::try_from(payload.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "content IPC frame length exceeds its wire type",
        )
    })?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

pub(in crate::native_engine::content_process) fn read_sync_frame(
    mut reader: impl Read,
) -> std::io::Result<Vec<u8>> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_CONTENT_IPC_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "content IPC frame exceeds its byte limit",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}

pub(in crate::native_engine::content_process) fn forward_content_ipc_frames(
    mut reader: impl Read,
    request_sender: mpsc::Sender<Result<Vec<u8>, NativeEngineError>>,
    dialog_waiting: Arc<AtomicBool>,
    dialog_sender: std::sync::mpsc::Sender<Result<NativeWorkerDialogDecision, String>>,
    sync_xhr_replies: Arc<Mutex<BTreeMap<u64, std::sync::mpsc::SyncSender<Vec<u8>>>>>,
) {
    loop {
        let payload = match read_sync_frame(&mut reader) {
            Ok(payload) => payload,
            Err(_) => {
                if let Ok(mut replies) = sync_xhr_replies.lock() {
                    replies.clear();
                }
                if dialog_waiting.swap(false, Ordering::AcqRel) {
                    let _ = dialog_sender.send(Err(
                        "content process pipe closed before its dialog decision".into(),
                    ));
                } else {
                    let _ = request_sender.blocking_send(Err(NativeEngineError::Worker {
                        operation: "read content IPC".into(),
                        reason: "content process exited or closed its pipe".into(),
                    }));
                }
                break;
            }
        };
        if let Ok(response) = serde_json::from_slice::<Value>(&payload)
            && response.get("kind").and_then(Value::as_str) == Some("parent_sync_xhr_response")
        {
            let sync_id = response.get("sync_id").and_then(Value::as_u64);
            let reply = sync_id.and_then(|sync_id| {
                sync_xhr_replies
                    .lock()
                    .ok()
                    .and_then(|mut replies| replies.remove(&sync_id))
            });
            if let Some(reply) = reply {
                let _ = reply.send(payload);
                continue;
            }
            if let Ok(mut replies) = sync_xhr_replies.lock() {
                // Wake every blocked broker call before terminating the reader.
                replies.clear();
            }
            let _ = request_sender.blocking_send(Err(NativeEngineError::worker_failure(
                "route parent-brokered synchronous XHR response",
                NativeWorkerFailureKind::Protocol,
                "parent returned a response for an unknown synchronous XHR request",
            )));
            break;
        }
        if dialog_waiting.swap(false, Ordering::AcqRel) {
            let decision = serde_json::from_slice(&payload)
                .map_err(|_| "native dialog decision was malformed".to_owned());
            if dialog_sender.send(decision).is_err() {
                break;
            }
        } else if request_sender.blocking_send(Ok(payload)).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod document_snapshot_tests {
    use super::*;

    #[test]
    fn compressed_document_snapshot_round_trips_with_bounded_decoding() {
        let document = br#"{"nodes":[{"kind":"Element","state":{}},{"kind":"Text","value":"repeated document content"}]}"#
            .repeat(128);
        let encoded = encode_document_snapshot_base64(&document);
        let packed = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        assert!(packed.len() < document.len());
        assert_eq!(
            decode_document_snapshot_base64(&encoded, "test snapshot").unwrap(),
            document
        );
    }

    #[test]
    fn raw_legacy_document_snapshot_still_decodes() {
        let document = b"{\"legacy\":true}";
        let encoded = base64::engine::general_purpose::STANDARD.encode(document);
        assert_eq!(
            decode_document_snapshot_base64(&encoded, "test snapshot").unwrap(),
            document
        );
    }

    #[test]
    fn compressed_document_snapshot_cannot_expand_past_document_budget() {
        let oversized = vec![b'x'; MAX_CONTENT_DOCUMENT_WIRE_BYTES + 1];
        let encoded = encode_document_snapshot_base64(&oversized);
        assert!(encoded.len() < MAX_CONTENT_IPC_FRAME_BYTES);
        assert!(matches!(
            decode_document_snapshot_base64(&encoded, "test snapshot"),
            Err(NativeEngineError::LimitExceeded {
                resource,
                limit: MAX_CONTENT_DOCUMENT_WIRE_BYTES,
                actual,
            }) if resource == "content-process document snapshot"
                && actual == MAX_CONTENT_DOCUMENT_WIRE_BYTES + 1
        ));
    }
}
