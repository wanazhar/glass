use super::config::{
    NativeEngineLimits, Viewport, is_network_url, validate_url_text, without_fragment,
};
use super::dom::{NativeDocument, NativeDocumentWire, NativeNodeId, NativePageScriptSource};
use super::error::{NativeEngineError, NativeWorkerFailureKind};
use super::interaction::{MAX_NATIVE_EFFECTS, NativeEventKind, validate_native_edit_key};
use super::javascript::{
    MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_SCRIPT_BYTES, NativeJavaScriptRuntime, NativePageScript,
    NativeScriptCommand, NativeScriptEvaluation, execute_page_scripts, host_event_script,
    host_key_event_script, literal_dynamic_module_specifiers, static_module_specifiers,
};
use super::origin::NativeOrigin;
use super::resource_loader::{
    NativeFetchResponse, NativeNavigationMethod, NativeNavigationRequest, NativeResourceLoader,
};
#[cfg(windows)]
use super::sandbox::NativeContentSandbox;
use super::sandbox::prepare_worker_command;
use base64::Engine as _;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::time::timeout;
use url::Url;

const MAX_CONTENT_IPC_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTENT_DOCUMENT_WIRE_BYTES: usize = 2 * 1024 * 1024;
const CONTENT_WORKER_PROTOCOL_VERSION: u64 = 1;
const CONTENT_PROCESS_LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const CONTENT_PROCESS_MUTATION_TIMEOUT: Duration = Duration::from_secs(5);
const CONTENT_PROCESS_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONTENT_STYLESHEETS: usize = 16;
const MAX_CONTENT_STYLESHEET_BYTES: usize = 512 * 1024;

pub(crate) struct NativeContentLoad {
    pub(crate) url: String,
    pub(crate) origin: NativeOrigin,
    pub(crate) document: NativeDocumentWire,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeContentEvent {
    pub(crate) node_index: u32,
    pub(crate) kind: NativeEventKind,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeContentNavigation {
    pub(crate) node_index: u32,
    pub(crate) href: String,
}

pub(crate) struct NativeContentMutation {
    pub(crate) document: NativeDocumentWire,
    pub(crate) events: Vec<NativeContentEvent>,
    pub(crate) navigation: Option<NativeContentNavigation>,
}

pub(crate) struct NativeContentScriptResult {
    pub(crate) value: Value,
    pub(crate) mutation: Option<NativeContentMutation>,
}

/// Process-backed lifecycle and bounded document-transfer channel for one
/// native content runtime.
pub(crate) struct NativeContentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    next_request_id: u64,
    healthy: bool,
    failure_kind: Option<NativeWorkerFailureKind>,
    #[cfg(windows)]
    sandbox: NativeContentSandbox,
}

impl NativeContentProcess {
    pub(crate) async fn spawn() -> Result<Self, NativeEngineError> {
        let path = worker_binary_path()?;
        let (mut command, mut sandbox) = prepare_worker_command(&path)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "spawn content process",
                    NativeWorkerFailureKind::Spawn,
                    "native content worker could not be started",
                )
            })?;
        if let Err(error) = sandbox.attach(&child) {
            let _ = child.start_kill();
            return Err(error);
        }
        let stdin = child.stdin.take().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "spawn content process",
                NativeWorkerFailureKind::Transport,
                "native content worker stdin is unavailable",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "spawn content process",
                NativeWorkerFailureKind::Transport,
                "native content worker stdout is unavailable",
            )
        })?;
        let mut process = Self {
            child,
            stdin,
            stdout,
            next_request_id: 1,
            healthy: true,
            failure_kind: None,
            #[cfg(windows)]
            sandbox,
        };
        let id = process.next_id();
        let response = process
            .exchange(json!({
                "kind": "ping",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        require_response_kind(&response, "pong", id, "content process ping")?;
        if response.get("protocol").and_then(Value::as_u64) != Some(CONTENT_WORKER_PROTOCOL_VERSION)
        {
            return Err(NativeEngineError::worker_failure(
                "content process ping",
                NativeWorkerFailureKind::Protocol,
                "content process protocol version is unsupported",
            ));
        }
        Ok(process)
    }

    pub(crate) async fn start(&mut self) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "start",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        let result = require_response_kind(&response, "started", id, "content process start");
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn commit(&mut self) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "commit",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        let result = require_response_kind(&response, "committed", id, "content process commit");
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn load(
        &mut self,
        navigation: &NativeNavigationRequest,
        limits: &NativeEngineLimits,
        viewport: Viewport,
        referrer: Option<&str>,
    ) -> Result<NativeContentLoad, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange(json!({
                "kind": "load",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "url": navigation.url,
                "method": match navigation.method {
                    NativeNavigationMethod::Get => "GET",
                    NativeNavigationMethod::Post => "POST",
                },
                "body": navigation.body,
                "referrer": referrer,
                "max_document_bytes": limits.max_document_bytes,
                "max_nodes": limits.max_nodes,
                "max_dom_depth": limits.max_dom_depth,
                "max_text_bytes": limits.max_text_bytes,
                "viewport_width": viewport.width,
                "viewport_height": viewport.height,
                "viewport_device_scale_factor_milli": viewport.device_scale_factor_milli,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process load",
                    NativeWorkerFailureKind::Timeout,
                    "content process load exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process load".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the document load")
                    .into(),
            });
        }
        let result = decode_loaded_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn mutate_click_with_event_preflight(
        &mut self,
        node_index: u32,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_click_preflight",
            json!({"node_index": node_index}),
        )
        .await
    }

    pub(crate) async fn mutate_type_with_event_bridge(
        &mut self,
        node_index: u32,
        text: String,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_type_events",
            json!({"node_index": node_index, "text": text}),
        )
        .await
    }

    pub(crate) async fn mutate_key_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_events",
            json!({"node_index": node_index, "key": key}),
        )
        .await
    }

    pub(crate) async fn fetch(
        &mut self,
        document_url: &str,
        href: &str,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange(json!({
                "kind": "fetch",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "document_url": document_url,
                "href": href,
                "credentials": credentials,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process fetch",
                    NativeWorkerFailureKind::Timeout,
                    "content process fetch exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process fetch".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the fetch")
                    .into(),
            });
        }
        let result = decode_fetch_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn evaluate(
        &mut self,
        source: &str,
    ) -> Result<NativeContentScriptResult, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "script",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "source": source,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process script",
                    NativeWorkerFailureKind::Timeout,
                    "content process script exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the script")
                    .into(),
            });
        }
        decode_script_response(&response, id)
    }

    async fn mutate_with_request_kind(
        &mut self,
        id: u64,
        request_kind: &str,
        action: Value,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let response = match timeout(
            CONTENT_PROCESS_MUTATION_TIMEOUT,
            self.exchange(json!({
                "kind": request_kind,
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "action": action,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process mutation",
                    NativeWorkerFailureKind::Timeout,
                    "content process mutation exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            self.mark_failed(NativeWorkerFailureKind::Rejected);
            let _ = self.child.start_kill();
            return Err(NativeEngineError::worker_failure(
                "content process mutation",
                NativeWorkerFailureKind::Rejected,
                response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the mutation"),
            ));
        }
        let result = decode_mutated_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub(crate) fn failure_kind(&self) -> Option<NativeWorkerFailureKind> {
        self.failure_kind
    }

    pub(crate) async fn close(mut self) -> Result<(), NativeEngineError> {
        if let Ok(Some(_)) = self.child.try_wait() {
            self.mark_failed(NativeWorkerFailureKind::Exited);
            return Ok(());
        }
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "close",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        require_response_kind(&response, "closed", id, "content process close")?;
        match timeout(Duration::from_secs(1), self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(NativeEngineError::Worker {
                operation: "wait for content process".into(),
                reason: "content process did not report a valid exit".into(),
            }),
            Err(_) => {
                let _ = self.child.start_kill();
                Ok(())
            }
        }
    }

    async fn exchange(&mut self, request: Value) -> Result<Value, NativeEngineError> {
        let result = self.exchange_inner(request).await;
        match result {
            Ok(response) => Ok(response),
            Err(error) => {
                let kind = match self.child.try_wait() {
                    Ok(Some(_)) => NativeWorkerFailureKind::Exited,
                    Ok(None) | Err(_) => NativeWorkerFailureKind::Transport,
                };
                self.mark_failed(kind);
                Err(NativeEngineError::worker_failure(
                    "content process IPC",
                    kind,
                    error.to_string(),
                ))
            }
        }
    }

    fn mark_failed(&mut self, kind: NativeWorkerFailureKind) {
        self.healthy = false;
        self.failure_kind = Some(kind);
    }

    async fn exchange_inner(&mut self, request: Value) -> Result<Value, NativeEngineError> {
        let payload = serde_json::to_vec(&request).map_err(|_| NativeEngineError::Worker {
            operation: "encode content IPC".into(),
            reason: "content process request could not be encoded".into(),
        })?;
        write_frame(&mut self.stdin, &payload).await?;
        let response = read_frame(&mut self.stdout).await?;
        serde_json::from_slice(&response).map_err(|_| NativeEngineError::Worker {
            operation: "decode content IPC".into(),
            reason: "content process returned an invalid response".into(),
        })
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }
}

impl Drop for NativeContentProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    payload: &[u8],
) -> Result<(), NativeEngineError> {
    if payload.len() > MAX_CONTENT_IPC_FRAME_BYTES {
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

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> Result<Vec<u8>, NativeEngineError> {
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

fn require_response_kind(
    response: &Value,
    expected: &str,
    expected_id: u64,
    operation: &str,
) -> Result<(), NativeEngineError> {
    if response.get("kind").and_then(Value::as_str) == Some(expected)
        && response.get("id").and_then(Value::as_u64) == Some(expected_id)
    {
        return Ok(());
    }
    Err(NativeEngineError::worker_failure(
        operation,
        NativeWorkerFailureKind::Protocol,
        "content process rejected the typed command",
    ))
}

fn decode_loaded_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentLoad, NativeEngineError> {
    require_response_kind(response, "loaded", id, "content process load")?;
    let url =
        response
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process load".into(),
                reason: "content process omitted the final URL".into(),
            })?;
    validate_url_text("content process final URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned a non-HTTP(S) final URL".into(),
        });
    }
    let encoded_document = response
        .get("document_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process omitted the document snapshot".into(),
        })?;
    let document_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded_document)
        .map_err(|_| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned an invalid document snapshot".into(),
        })?;
    if document_bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            document_bytes.len(),
        ));
    }
    let document = decode_document_wire(&document_bytes, "decode content process load")?;
    let origin_url =
        url::Url::parse(without_fragment(url)).map_err(|_| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned invalid final URL syntax".into(),
        })?;
    let origin = NativeOrigin::from_url(&origin_url)?;
    Ok(NativeContentLoad {
        url: url.into(),
        origin,
        document,
    })
}

fn decode_mutated_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentMutation, NativeEngineError> {
    require_response_kind(response, "mutated", id, "content process mutation")?;
    decode_mutation_payload(response, "decode content process mutation")
}

fn decode_mutation_payload(
    response: &Value,
    operation: &str,
) -> Result<NativeContentMutation, NativeEngineError> {
    let encoded_document = response
        .get("document_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process omitted the mutated document snapshot".into(),
        })?;
    let document_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded_document)
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid document snapshot".into(),
        })?;
    let document = decode_document_wire(&document_bytes, operation)?;
    let event_values = response
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process omitted mutation effects".into(),
        })?;
    if event_values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process mutation effects",
            MAX_NATIVE_EFFECTS,
            event_values.len(),
        ));
    }
    let mut events = Vec::with_capacity(event_values.len());
    for value in event_values {
        let node_index = value
            .get("node_index")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid mutation node".into(),
            })?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(parse_event_kind)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid mutation effect".into(),
            })?;
        events.push(NativeContentEvent { node_index, kind });
    }
    let navigation = response
        .get("navigation")
        .filter(|value| !value.is_null())
        .map(|value| {
            let node_index = value
                .get("node_index")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned an invalid navigation node".into(),
                })?;
            let href = value.get("href").and_then(Value::as_str).ok_or_else(|| {
                NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned an invalid navigation href".into(),
                }
            })?;
            validate_url_text("content process navigation href", href)?;
            Ok(NativeContentNavigation {
                node_index,
                href: href.to_owned(),
            })
        })
        .transpose()?;
    Ok(NativeContentMutation {
        document,
        events,
        navigation,
    })
}

fn decode_fetch_response(
    response: &Value,
    id: u64,
) -> Result<NativeFetchResponse, NativeEngineError> {
    require_response_kind(response, "fetched", id, "content process fetch")?;
    let url =
        response
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process omitted the fetch URL".into(),
            })?;
    validate_url_text("content process fetch URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned a non-HTTP(S) fetch URL".into(),
        });
    }
    let status = response
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned an invalid HTTP status".into(),
        })?;
    let content_type = response
        .get("content_type")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "decode content process fetch".into(),
                    reason: "content process returned an invalid content type".into(),
                })
        })
        .transpose()?;
    let encoded_body = response
        .get("body_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process omitted the fetch body".into(),
        })?;
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded_body)
        .map_err(|_| NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned an invalid fetch body".into(),
        })?;
    if body.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process fetch response",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            body.len(),
        ));
    }
    Ok(NativeFetchResponse {
        url: url.to_owned(),
        status,
        content_type,
        body,
    })
}

fn decode_script_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentScriptResult, NativeEngineError> {
    require_response_kind(response, "evaluated", id, "content process script")?;
    let value = response
        .get("value")
        .cloned()
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process script".into(),
            reason: "content process omitted the script result".into(),
        })?;
    let encoded = serde_json::to_vec(&value).map_err(|_| NativeEngineError::Worker {
        operation: "decode content process script".into(),
        reason: "content process returned an unserializable script result".into(),
    })?;
    if encoded.len() > super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES {
        return Err(NativeEngineError::limit(
            "content-process script result",
            super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
            encoded.len(),
        ));
    }
    let has_document = response.get("document_base64").is_some();
    let has_events = response.get("events").is_some();
    let mutation = if has_document || has_events {
        Some(decode_mutation_payload(
            response,
            "decode content process script",
        )?)
    } else {
        None
    };
    Ok(NativeContentScriptResult { value, mutation })
}

fn decode_document_wire(
    document_bytes: &[u8],
    operation: &str,
) -> Result<NativeDocumentWire, NativeEngineError> {
    if document_bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            document_bytes.len(),
        ));
    }
    serde_json::from_slice(document_bytes).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned an invalid document snapshot".into(),
    })
}

#[doc(hidden)]
pub async fn run_native_content_worker() -> Result<(), NativeEngineError> {
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut running = false;
    let mut document = None;
    let mut document_url = None;
    let mut document_origin = None;
    let mut viewport = Viewport::default();
    let mut resource_loader = None;
    let mut javascript_runtime = None;
    loop {
        let payload = read_frame(&mut stdin).await?;
        let request: Value =
            serde_json::from_slice(&payload).map_err(|_| NativeEngineError::Worker {
                operation: "decode content IPC".into(),
                reason: "content process received an invalid request".into(),
            })?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let kind = request
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let response = match kind {
            "ping" if protocol_matches(&request) => {
                json!({"kind":"pong","id":id,"protocol":CONTENT_WORKER_PROTOCOL_VERSION})
            }
            "start" if protocol_matches(&request) && !running => {
                running = true;
                json!({"kind":"started","id":id})
            }
            "commit" if protocol_matches(&request) && running => {
                json!({"kind":"committed","id":id})
            }
            "load" if protocol_matches(&request) && running => {
                match load_content_resource(&request, &mut resource_loader).await {
                    Ok((resource, mut parsed, loaded_viewport, script_sources)) => {
                        let mut script_runtime = None;
                        let response = match execute_page_scripts(
                            &mut parsed,
                            &mut script_runtime,
                            &script_sources,
                            &resource.url,
                            &resource.origin,
                            loaded_viewport,
                        ) {
                            Ok(()) => {
                                let document_wire = parsed.to_content_wire();
                                document = Some(parsed);
                                document_url = Some(resource.url.clone());
                                document_origin = Some(resource.origin.clone());
                                viewport = loaded_viewport;
                                javascript_runtime = script_runtime;
                                json!({
                                    "kind": "loaded",
                                    "id": id,
                                    "url": resource.url,
                                    "document_base64": base64::engine::general_purpose::STANDARD
                                        .encode(serde_json::to_vec(&document_wire).unwrap_or_default()),
                                })
                            }
                            Err(error) => content_error_response(id, error),
                        };
                        response
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "fetch" if protocol_matches(&request) && running => {
                let Some(_current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let document_url = request
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process fetch owner URL",
                            "must be text",
                        )
                    })?;
                let href = request.get("href").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process fetch URL", "must be text")
                })?;
                let credentials = request
                    .get("credentials")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let Some(loader) = resource_loader.as_mut() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "content process has no resource loader".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                match loader.fetch_async(document_url, href, credentials).await {
                    Ok(fetch) => {
                        json!({
                            "kind": "fetched",
                            "id": id,
                            "url": fetch.url,
                            "status": fetch.status,
                            "content_type": fetch.content_type,
                            "body_base64": base64::engine::general_purpose::STANDARD
                                .encode(fetch.body),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "script" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let source = request
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process script source", "must be text")
                    })?;
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new() {
                        Ok(runtime) => javascript_runtime = Some(runtime),
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                let Some(document_url) = document_url.as_deref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                match runtime.evaluate(source, current, document_url, document_origin, viewport) {
                    Ok(NativeScriptEvaluation { value, commands }) if commands.is_empty() => {
                        json!({"kind":"evaluated","id":id,"value":value})
                    }
                    Ok(NativeScriptEvaluation { value, commands }) => {
                        match mutate_script_document(
                            current,
                            runtime,
                            document_url,
                            document_origin,
                            viewport,
                            &commands,
                        ) {
                            Ok((next, mutation)) => {
                                document = Some(next);
                                json!({
                                    "kind": "evaluated",
                                    "id": id,
                                    "value": value,
                                    "document_base64": base64::engine::general_purpose::STANDARD
                                        .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                                    "events": mutation.events.iter().map(|event| json!({
                                        "node_index": event.node_index,
                                        "kind": event_kind_text(event.kind),
                                    })).collect::<Vec<_>>(),
                                    "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                                        "node_index": navigation.node_index,
                                        "href": navigation.href,
                                    })),
                                })
                            }
                            Err(error) => content_error_response(id, error),
                        }
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_click_preflight" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let node_index = request
                    .get("action")
                    .and_then(|action| action.get("node_index"))
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process click target",
                            "must be a uint32",
                        )
                    })?;
                let Some(document_url) = document_url.as_deref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new() {
                        Ok(runtime) => javascript_runtime = Some(runtime),
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                match mutate_click_with_event_preflight(
                    current,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    node_index,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                                "node_index": navigation.node_index,
                                "href": navigation.href,
                            })),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_type_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process type action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process type target",
                            "must be a uint32",
                        )
                    })?;
                let text = action.get("text").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process type text", "must be text")
                })?;
                let Some(document_url) = document_url.as_deref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new() {
                        Ok(runtime) => javascript_runtime = Some(runtime),
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                match mutate_type_with_event_bridge(
                    current,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    node_index,
                    text,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_key_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process key action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process key target", "must be a uint32")
                    })?;
                let key = action.get("key").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process key", "must be text")
                })?;
                if let Err(error) = validate_native_edit_key(key) {
                    let response = content_error_response(id, error);
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let Some(document_url) = document_url.as_deref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new() {
                        Ok(runtime) => javascript_runtime = Some(runtime),
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                match mutate_key_with_event_bridge(
                    current,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    node_index,
                    key,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "close" if protocol_matches(&request) => {
                write_value_frame(&mut stdout, &json!({"kind":"closed","id":id})).await?;
                return Ok(());
            }
            _ => content_error_response(
                id,
                NativeEngineError::Worker {
                    operation: "content process command".into(),
                    reason: "content process rejected the typed command".into(),
                },
            ),
        };
        write_value_frame(&mut stdout, &response).await?;
    }
}

async fn load_content_resource(
    request: &Value,
    resource_loader: &mut Option<NativeResourceLoader>,
) -> Result<
    (
        NativeContentLoad,
        NativeDocument,
        Viewport,
        Vec<NativePageScript>,
    ),
    NativeEngineError,
> {
    let url = request
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::invalid("content-process URL", "must be text"))?;
    validate_url_text("content-process URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "content process accepts only HTTP(S) document URLs".into(),
        });
    }
    let method = match request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
    {
        "GET" => NativeNavigationMethod::Get,
        "POST" => NativeNavigationMethod::Post,
        _ => {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "content process supports only GET and POST document navigation".into(),
            });
        }
    };
    let body = request
        .get("body")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation body",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    let navigation = match method {
        NativeNavigationMethod::Get => {
            if body.is_some() {
                return Err(NativeEngineError::invalid(
                    "content-process GET body",
                    "must be null",
                ));
            }
            NativeNavigationRequest::get(url)
        }
        NativeNavigationMethod::Post => NativeNavigationRequest::post(
            url,
            body.ok_or_else(|| {
                NativeEngineError::invalid("content-process POST body", "must be present")
            })?
            .to_owned(),
        )?,
    };
    let max_document_bytes = request
        .get("max_document_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "content-process document limit",
                "must be a positive integer",
            )
        })?;
    let limits = NativeEngineLimits {
        max_document_bytes,
        max_nodes: request
            .get("max_nodes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_nodes),
        max_dom_depth: request
            .get("max_dom_depth")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_dom_depth),
        max_text_bytes: request
            .get("max_text_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_text_bytes),
        ..NativeEngineLimits::default()
    };
    let viewport = Viewport {
        width: request
            .get("viewport_width")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().width),
        height: request
            .get("viewport_height")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().height),
        device_scale_factor_milli: request
            .get("viewport_device_scale_factor_milli")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().device_scale_factor_milli),
    };
    viewport.validate()?;
    let referrer = request
        .get("referrer")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid("content-process referrer", "must be text or null")
            })
        })
        .transpose()?;
    let loader = match resource_loader {
        Some(loader) if loader.max_document_bytes() == max_document_bytes => loader,
        Some(_) => {
            return Err(NativeEngineError::invalid(
                "content-process document limit",
                "cannot change the document limit after the content process starts",
            ));
        }
        None => {
            *resource_loader = Some(NativeResourceLoader::for_content_process(
                max_document_bytes,
            )?);
            resource_loader
                .as_mut()
                .expect("content-process resource loader was just initialized")
        }
    };
    let resource = loader
        .load_async_request_with_referrer(&navigation, referrer)
        .await?;
    let discovery = NativeDocument::parse(&resource.body, &limits)?;
    let mut external_stylesheets = Vec::new();
    for href in discovery
        .external_stylesheet_hrefs()
        .into_iter()
        .take(MAX_CONTENT_STYLESHEETS)
    {
        if let Some(stylesheet) = loader.load_stylesheet_async(&resource.url, &href).await? {
            let next_len = external_stylesheets
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(stylesheet.len());
            if next_len > MAX_CONTENT_STYLESHEET_BYTES {
                return Err(NativeEngineError::limit(
                    "CSS subresources",
                    MAX_CONTENT_STYLESHEET_BYTES,
                    next_len,
                ));
            }
            external_stylesheets.push(stylesheet);
        }
    }
    let document =
        NativeDocument::parse_with_stylesheets(&resource.body, &limits, &external_stylesheets, 1)?;
    let script_sources = load_page_script_sources(&document, loader, &resource.url).await?;
    let wire = document.to_content_wire();
    Ok((
        NativeContentLoad {
            url: resource.url,
            origin: resource.origin,
            document: wire,
        },
        document,
        viewport,
        script_sources,
    ))
}

async fn load_page_script_sources(
    document: &NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<Vec<NativePageScript>, NativeEngineError> {
    let mut sources = Vec::new();
    for (index, script) in document
        .page_script_sources(
            super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
            MAX_NATIVE_SCRIPT_BYTES,
        )
        .into_iter()
        .enumerate()
    {
        match script {
            NativePageScriptSource::Inline(source) => {
                sources.push(NativePageScript::Classic { source });
            }
            NativePageScriptSource::ModuleInline(source) => {
                let name = format!("{document_url}#glass-inline-module-{index}");
                let mut seen = BTreeSet::new();
                seen.insert(name.clone());
                let mut total_bytes = source.len();
                sources.push(NativePageScript::Module {
                    name: name.clone(),
                    source: source.clone(),
                });
                load_module_dependencies(
                    document_url,
                    &name,
                    &source,
                    loader,
                    &mut sources,
                    &mut seen,
                    &mut total_bytes,
                )
                .await?;
            }
            NativePageScriptSource::External(href) => {
                if let Some(resource) = loader
                    .load_script_async(document_url, &href, MAX_NATIVE_SCRIPT_BYTES)
                    .await?
                {
                    sources.push(NativePageScript::Classic {
                        source: resource.body,
                    });
                }
            }
            NativePageScriptSource::ModuleExternal(href) => {
                if let Some(resource) = loader
                    .load_script_async(document_url, &href, MAX_NATIVE_SCRIPT_BYTES)
                    .await?
                {
                    let name = resource.url;
                    let source = resource.body;
                    let mut seen = BTreeSet::new();
                    seen.insert(name.clone());
                    let mut total_bytes = source.len();
                    sources.push(NativePageScript::Module {
                        name: name.clone(),
                        source: source.clone(),
                    });
                    load_module_dependencies(
                        document_url,
                        &name,
                        &source,
                        loader,
                        &mut sources,
                        &mut seen,
                        &mut total_bytes,
                    )
                    .await?;
                }
            }
        }
    }
    Ok(sources)
}

async fn load_module_dependencies(
    owner_url: &str,
    module_url: &str,
    source: &str,
    loader: &mut NativeResourceLoader,
    scripts: &mut Vec<NativePageScript>,
    seen: &mut BTreeSet<String>,
    total_bytes: &mut usize,
) -> Result<(), NativeEngineError> {
    let mut pending = vec![(module_url.to_owned(), source.to_owned())];
    while let Some((current_url, current_source)) = pending.pop() {
        let mut specifiers = static_module_specifiers(&current_source)?;
        specifiers.extend(literal_dynamic_module_specifiers(&current_source));
        for specifier in specifiers {
            let Some(target) = resolve_module_specifier(&current_url, &specifier)? else {
                continue;
            };
            let Some(resource) = loader
                .load_script_async(owner_url, &target, MAX_NATIVE_SCRIPT_BYTES)
                .await?
            else {
                continue;
            };
            let name = resource.url;
            if !seen.insert(name.clone()) {
                continue;
            }
            if seen.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module graph entries",
                    MAX_NATIVE_MODULE_IMPORTS,
                    seen.len(),
                ));
            }
            let source = resource.body;
            *total_bytes = total_bytes.saturating_add(source.len());
            if *total_bytes > MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS) {
                return Err(NativeEngineError::limit(
                    "module graph bytes",
                    MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS),
                    *total_bytes,
                ));
            }
            scripts.push(NativePageScript::ModuleDependency {
                name: name.clone(),
                source: source.clone(),
            });
            pending.push((name, source));
        }
    }
    Ok(())
}

fn resolve_module_specifier(
    module_url: &str,
    specifier: &str,
) -> Result<Option<String>, NativeEngineError> {
    let is_absolute = specifier.starts_with("http://") || specifier.starts_with("https://");
    if !is_absolute
        && !specifier.starts_with("./")
        && !specifier.starts_with("../")
        && !specifier.starts_with('/')
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "bare module specifiers require an import map".into(),
        });
    }
    let base = Url::parse(without_fragment(module_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "module owner URL is not valid URL syntax".into(),
        }
    })?;
    let mut target = if is_absolute {
        Url::parse(specifier)
    } else {
        base.join(specifier)
    }
    .map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "module specifier could not be resolved against its owner".into(),
    })?;
    target.set_fragment(None);
    if !is_network_url(target.as_str()) {
        return Ok(None);
    }
    Ok(Some(target.to_string()))
}

fn mutate_click_with_event_preflight(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut events = next.apply_script_focus(node_id)?;
    let focus_metadata = events
        .iter()
        .map(|(node, kind)| (node.index(), *kind))
        .collect::<Vec<_>>();
    if let Some(source) = host_event_script(&focus_metadata)? {
        let evaluation =
            runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
        events.extend(next.apply_script_commands(&evaluation.commands)?);
    }

    let click_source =
        host_event_script(&[(node_index, NativeEventKind::Click)])?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process click preflight".into(),
                reason: "native click event source was empty".into(),
            }
        })?;
    let click_evaluation = runtime.evaluate(
        &click_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    let click_allowed = click_evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process click preflight".into(),
            reason: "native click event result was invalid".into(),
        })?;
    events.extend(next.apply_script_commands(&click_evaluation.commands)?);
    let mut navigation = None;
    if click_allowed {
        events.extend(next.apply_click(node_id)?);
        if let Some(form_id) = next.submit_control_form(node_id) {
            if dispatch_submit_event(
                &mut next,
                runtime,
                document_url,
                document_origin,
                viewport,
                form_id,
                &mut events,
            )? {
                navigation = Some(NativeContentNavigation {
                    node_index: form_id.index(),
                    href: next.form_submission_request(form_id, document_url)?.url,
                });
            }
        }
    } else {
        events.push((node_id, NativeEventKind::Click));
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process click event effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation,
    };
    Ok((next, mutation))
}

fn dispatch_submit_event(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    form_id: NativeNodeId,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
) -> Result<bool, NativeEngineError> {
    let source =
        host_event_script(&[(form_id.index(), NativeEventKind::Submit)])?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process submit event".into(),
                reason: "native submit event source was empty".into(),
            }
        })?;
    let evaluation =
        runtime.evaluate(&source, document, document_url, document_origin, viewport)?;
    let allowed = evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process submit event".into(),
            reason: "native submit event result was invalid".into(),
        })?;
    events.push((form_id, NativeEventKind::Submit));
    events.extend(document.apply_script_commands(&evaluation.commands)?);
    Ok(allowed)
}

fn mutate_type_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    text: &str,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut events = next.apply_type(node_id, text)?;
    let default_events = events.clone();
    for (event_node, event_kind) in default_events {
        let source = host_event_script(&[(event_node.index(), event_kind)])?;
        let Some(source) = source else {
            continue;
        };
        let evaluation =
            runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
        events.extend(next.apply_script_commands(&evaluation.commands)?);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process type event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
    };
    Ok((next, mutation))
}

fn mutate_key_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_edit_key(key)?;
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    if current.focused_text_control()? != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "key press target is not the focused text control".into(),
        });
    }
    let mut next = current.clone();
    let mut events = vec![(node_id, NativeEventKind::KeyDown)];
    let keydown_source = host_key_event_script(node_index, NativeEventKind::KeyDown, key)?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keydown event bridge".into(),
            reason: "native keydown event source was empty".into(),
        })?;
    let keydown = runtime.evaluate(
        &keydown_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    let keydown_allowed = keydown
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keydown event bridge".into(),
            reason: "native keydown event result was invalid".into(),
        })?;
    events.extend(next.apply_script_commands(&keydown.commands)?);

    if keydown_allowed
        && next
            .focused_text_control()
            .is_ok_and(|focused| focused == node_id)
    {
        let input_events = next.apply_key_press(node_id, key)?;
        events.extend(input_events.clone());
        for (event_node, event_kind) in input_events {
            let source = host_event_script(&[(event_node.index(), event_kind)])?;
            let Some(source) = source else {
                continue;
            };
            let evaluation =
                runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
            events.extend(next.apply_script_commands(&evaluation.commands)?);
        }
    }

    events.push((node_id, NativeEventKind::KeyUp));
    let keyup_source =
        host_key_event_script(node_index, NativeEventKind::KeyUp, key)?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process keyup event bridge".into(),
                reason: "native keyup event source was empty".into(),
            }
        })?;
    let keyup = runtime.evaluate(
        &keyup_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    events.extend(next.apply_script_commands(&keyup.commands)?);
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process key press effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
    };
    Ok((next, mutation))
}

fn mutate_script_document(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    commands: &[NativeScriptCommand],
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let mut next = current.clone();
    let mut events = next.apply_script_commands_allowing_links(commands)?;
    let mut navigation = script_navigation_target(&next, document_url, commands)?;
    if let Some(ScriptNavigationTarget::Form {
        form_id,
        dispatch_submit: true,
        ..
    }) = navigation.as_ref()
    {
        if !dispatch_submit_event(
            &mut next,
            runtime,
            document_url,
            document_origin,
            viewport,
            *form_id,
            &mut events,
        )? {
            navigation = None;
        }
    }
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process script mutation effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: navigation
            .map(|navigation| match navigation {
                ScriptNavigationTarget::Link { node_index, href } => {
                    Ok(NativeContentNavigation { node_index, href })
                }
                ScriptNavigationTarget::Form {
                    form_id,
                    node_index,
                    ..
                } => Ok(NativeContentNavigation {
                    node_index,
                    href: next.form_submission_request(form_id, document_url)?.url,
                }),
            })
            .transpose()?,
    };
    Ok((next, mutation))
}

enum ScriptNavigationTarget {
    Link {
        node_index: u32,
        href: String,
    },
    Form {
        node_index: u32,
        form_id: NativeNodeId,
        dispatch_submit: bool,
    },
}

fn script_navigation_target(
    document: &NativeDocument,
    document_url: &str,
    commands: &[NativeScriptCommand],
) -> Result<Option<ScriptNavigationTarget>, NativeEngineError> {
    let mut navigation = None;
    for command in commands {
        let target = match command {
            NativeScriptCommand::Click { node_index } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                if let Some(href) = document.link_href(node_id).filter(|href| !href.is_empty()) {
                    Some(ScriptNavigationTarget::Link {
                        node_index: *node_index,
                        href: href.to_owned(),
                    })
                } else if let Some(form_id) = document.submit_control_form(node_id) {
                    document.form_submission_request(form_id, document_url)?;
                    Some(ScriptNavigationTarget::Form {
                        node_index: form_id.index(),
                        form_id,
                        dispatch_submit: true,
                    })
                } else {
                    None
                }
            }
            NativeScriptCommand::SubmitForm { node_index } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                document.form_submission_request(node_id, document_url)?;
                Some(ScriptNavigationTarget::Form {
                    node_index: *node_index,
                    form_id: node_id,
                    dispatch_submit: false,
                })
            }
            NativeScriptCommand::RequestSubmitForm { node_index } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                document.form_submission_request(node_id, document_url)?;
                Some(ScriptNavigationTarget::Form {
                    node_index: *node_index,
                    form_id: node_id,
                    dispatch_submit: true,
                })
            }
            _ => None,
        };
        let Some(target) = target else { continue };
        if navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one script batch cannot activate multiple navigations".into(),
            });
        }
        navigation = Some(target);
    }
    Ok(navigation)
}

fn event_kind_text(kind: NativeEventKind) -> &'static str {
    match kind {
        NativeEventKind::Blur => "blur",
        NativeEventKind::Focus => "focus",
        NativeEventKind::KeyDown => "keydown",
        NativeEventKind::KeyUp => "keyup",
        NativeEventKind::Submit => "submit",
        NativeEventKind::Click => "click",
        NativeEventKind::Input => "input",
        NativeEventKind::Change => "change",
        NativeEventKind::Scroll => "scroll",
    }
}

fn parse_event_kind(value: &str) -> Option<NativeEventKind> {
    match value {
        "blur" => Some(NativeEventKind::Blur),
        "focus" => Some(NativeEventKind::Focus),
        "keydown" => Some(NativeEventKind::KeyDown),
        "keyup" => Some(NativeEventKind::KeyUp),
        "submit" => Some(NativeEventKind::Submit),
        "click" => Some(NativeEventKind::Click),
        "input" => Some(NativeEventKind::Input),
        "change" => Some(NativeEventKind::Change),
        "scroll" => Some(NativeEventKind::Scroll),
        _ => None,
    }
}

async fn write_value_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &Value,
) -> Result<(), NativeEngineError> {
    let payload = serde_json::to_vec(value).map_err(|_| NativeEngineError::Worker {
        operation: "encode content IPC".into(),
        reason: "content process response could not be encoded".into(),
    })?;
    write_frame(writer, &payload).await
}

fn content_error_response(id: Value, error: NativeEngineError) -> Value {
    json!({
        "kind": "error",
        "id": id,
        "reason": error.to_string(),
    })
}

fn protocol_matches(request: &Value) -> bool {
    request.get("protocol").and_then(Value::as_u64) == Some(CONTENT_WORKER_PROTOCOL_VERSION)
}

fn worker_binary_path() -> Result<PathBuf, NativeEngineError> {
    if let Ok(path) = std::env::var("GLASS_NATIVE_CONTENT_WORKER") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let current = std::env::current_exe().map_err(|_| {
        NativeEngineError::worker_failure(
            "locate content process",
            NativeWorkerFailureKind::Spawn,
            "current executable path is unavailable",
        )
    })?;
    let name = if cfg!(windows) {
        "glass-native-content-worker.exe"
    } else {
        "glass-native-content-worker"
    };
    let mut directory = current.parent();
    for _ in 0..3 {
        let Some(parent) = directory else {
            break;
        };
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
        directory = parent.parent();
    }
    Err(NativeEngineError::worker_failure(
        "locate content process",
        NativeWorkerFailureKind::Spawn,
        "native content worker executable was not found; build the glass-native-content-worker binary",
    ))
}
