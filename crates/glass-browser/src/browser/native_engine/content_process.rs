use super::config::{NativeEngineLimits, is_network_url, validate_url_text, without_fragment};
use super::dom::{NativeDocument, NativeDocumentWire, NativeNodeId};
use super::error::NativeEngineError;
use super::interaction::{MAX_NATIVE_EFFECTS, NativeEventKind};
use super::origin::NativeOrigin;
use super::resource_loader::NativeResourceLoader;
use base64::Engine as _;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

const MAX_CONTENT_IPC_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTENT_DOCUMENT_WIRE_BYTES: usize = 2 * 1024 * 1024;
const CONTENT_WORKER_PROTOCOL_VERSION: u64 = 1;
const CONTENT_PROCESS_LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const CONTENT_PROCESS_MUTATION_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct NativeContentLoad {
    pub(crate) url: String,
    pub(crate) origin: NativeOrigin,
    pub(crate) document: NativeDocumentWire,
}

#[derive(Debug, Clone)]
pub(crate) enum NativeContentAction {
    Click { node_index: u32 },
    Type { node_index: u32, text: String },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeContentEvent {
    pub(crate) node_index: u32,
    pub(crate) kind: NativeEventKind,
}

pub(crate) struct NativeContentMutation {
    pub(crate) document: NativeDocumentWire,
    pub(crate) events: Vec<NativeContentEvent>,
}

/// Process-backed lifecycle and bounded document-transfer channel for one
/// native content runtime.
pub(crate) struct NativeContentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    next_request_id: u64,
    healthy: bool,
}

impl NativeContentProcess {
    pub(crate) async fn spawn() -> Result<Self, NativeEngineError> {
        let path = worker_binary_path()?;
        let mut child = Command::new(path)
            .arg("--native-content-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| NativeEngineError::Worker {
                operation: "spawn content process".into(),
                reason: "native content worker could not be started".into(),
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "spawn content process".into(),
                reason: "native content worker stdin is unavailable".into(),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "spawn content process".into(),
                reason: "native content worker stdout is unavailable".into(),
            })?;
        let mut process = Self {
            child,
            stdin,
            stdout,
            next_request_id: 1,
            healthy: true,
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
            return Err(NativeEngineError::Worker {
                operation: "content process ping".into(),
                reason: "content process protocol version is unsupported".into(),
            });
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
        require_response_kind(&response, "started", id, "content process start")
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
        require_response_kind(&response, "committed", id, "content process commit")
    }

    pub(crate) async fn load(
        &mut self,
        url: &str,
        limits: &NativeEngineLimits,
    ) -> Result<NativeContentLoad, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange(json!({
                "kind": "load",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "url": url,
                "max_document_bytes": limits.max_document_bytes,
                "max_nodes": limits.max_nodes,
                "max_dom_depth": limits.max_dom_depth,
                "max_text_bytes": limits.max_text_bytes,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.healthy = false;
                let _ = self.child.start_kill();
                return Err(NativeEngineError::Worker {
                    operation: "content process load".into(),
                    reason: "content process load exceeded its deadline".into(),
                });
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
            self.healthy = false;
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn mutate(
        &mut self,
        action: NativeContentAction,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        let action = match action {
            NativeContentAction::Click { node_index } => {
                json!({"kind":"click","node_index":node_index})
            }
            NativeContentAction::Type { node_index, text } => {
                json!({"kind":"type","node_index":node_index,"text":text})
            }
        };
        self.mutate_with_request(id, action).await
    }

    async fn mutate_with_request(
        &mut self,
        id: u64,
        action: Value,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let response = match timeout(
            CONTENT_PROCESS_MUTATION_TIMEOUT,
            self.exchange(json!({
                "kind": "mutate",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "action": action,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.healthy = false;
                let _ = self.child.start_kill();
                return Err(NativeEngineError::Worker {
                    operation: "content process mutation".into(),
                    reason: "content process mutation exceeded its deadline".into(),
                });
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            self.healthy = false;
            let _ = self.child.start_kill();
            return Err(NativeEngineError::Worker {
                operation: "content process mutation".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the mutation")
                    .into(),
            });
        }
        let result = decode_mutated_response(&response, id);
        if result.is_err() {
            self.healthy = false;
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub(crate) async fn close(mut self) -> Result<(), NativeEngineError> {
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
        if result.is_err() {
            self.healthy = false;
        }
        result
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
    Err(NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process rejected the typed command".into(),
    })
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
    let encoded_document = response
        .get("document_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process mutation".into(),
            reason: "content process omitted the mutated document snapshot".into(),
        })?;
    let document_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded_document)
        .map_err(|_| NativeEngineError::Worker {
            operation: "decode content process mutation".into(),
            reason: "content process returned an invalid document snapshot".into(),
        })?;
    let document = decode_document_wire(&document_bytes, "decode content process mutation")?;
    let event_values = response
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process mutation".into(),
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
                operation: "decode content process mutation".into(),
                reason: "content process returned an invalid mutation node".into(),
            })?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(parse_event_kind)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process mutation".into(),
                reason: "content process returned an invalid mutation effect".into(),
            })?;
        events.push(NativeContentEvent { node_index, kind });
    }
    Ok(NativeContentMutation { document, events })
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
                match load_content_resource(&request).await {
                    Ok((resource, parsed)) => {
                        document = Some(parsed);
                        json!({
                            "kind": "loaded",
                            "id": id,
                            "url": resource.url,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&resource.document).unwrap_or_default()),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process mutation".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                match mutate_content_document(current, &request) {
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
) -> Result<(NativeContentLoad, NativeDocument), NativeEngineError> {
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
    let resource = NativeResourceLoader::for_content_process(max_document_bytes)?
        .load_async(url)
        .await?;
    let document = NativeDocument::parse(&resource.body, &limits)?;
    let wire = document.to_content_wire();
    Ok((
        NativeContentLoad {
            url: resource.url,
            origin: resource.origin,
            document: wire,
        },
        document,
    ))
}

fn mutate_content_document(
    current: &NativeDocument,
    request: &Value,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let action = request
        .get("action")
        .ok_or_else(|| NativeEngineError::invalid("content-process action", "is required"))?;
    let node_index = action
        .get("node_index")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::invalid("content-process node", "must be a uint32"))?;
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let events = match action.get("kind").and_then(Value::as_str) {
        Some("click") => next.apply_click(node_id)?,
        Some("type") => {
            let text = action.get("text").and_then(Value::as_str).ok_or_else(|| {
                NativeEngineError::invalid("content-process text", "must be text")
            })?;
            next.apply_type(node_id, text)?
        }
        _ => {
            return Err(NativeEngineError::invalid(
                "content-process action",
                "must be click or type",
            ));
        }
    };
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process mutation effects",
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
    };
    Ok((next, mutation))
}

fn event_kind_text(kind: NativeEventKind) -> &'static str {
    match kind {
        NativeEventKind::Blur => "blur",
        NativeEventKind::Focus => "focus",
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
    let current = std::env::current_exe().map_err(|_| NativeEngineError::Worker {
        operation: "locate content process".into(),
        reason: "current executable path is unavailable".into(),
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
    Err(NativeEngineError::Worker {
        operation: "locate content process".into(),
        reason: "native content worker executable was not found; build the glass-native-content-worker binary".into(),
    })
}
