//! EventSource framing and WebSocket request validation.

use super::*;

#[derive(Default)]
pub struct NativeEventSourceParser {
    pub(super) buffer: Vec<u8>,
    pub(super) data: String,
    pub(super) event: String,
    pub last_event_id: String,
    pub retry: Duration,
    pub(super) skip_lf_after_cr: bool,
}

impl NativeEventSourceParser {
    pub fn with_state(last_event_id: String, retry: Duration) -> Self {
        Self {
            last_event_id,
            retry,
            ..Self::default()
        }
    }
}

pub struct NativeEventSourceMessage {
    pub event: String,
    pub data: String,
    pub last_event_id: String,
}

pub fn parse_event_source_chunk(
    parser: &mut NativeEventSourceParser,
    chunk: &[u8],
) -> Result<Vec<NativeEventSourceMessage>, NativeEngineError> {
    let next_len = parser.buffer.len().saturating_add(chunk.len());
    if next_len > MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES + MAX_NATIVE_EVENTSOURCE_FIELD_BYTES {
        return Err(NativeEngineError::limit(
            "native EventSource line buffer",
            MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES + MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
            next_len,
        ));
    }
    parser.buffer.extend_from_slice(chunk);
    let mut messages = Vec::new();
    loop {
        if parser.skip_lf_after_cr {
            match parser.buffer.first() {
                Some(b'\n') => {
                    parser.buffer.remove(0);
                    parser.skip_lf_after_cr = false;
                }
                Some(_) => parser.skip_lf_after_cr = false,
                None => break,
            }
        }
        let Some(line_end) = parser
            .buffer
            .iter()
            .position(|byte| matches!(*byte, b'\r' | b'\n'))
        else {
            break;
        };
        let is_cr = parser.buffer[line_end] == b'\r';
        let terminator_len = if is_cr && parser.buffer.get(line_end + 1) == Some(&b'\n') {
            2
        } else {
            if is_cr {
                parser.skip_lf_after_cr = true;
            }
            1
        };
        let line = parser.buffer.drain(..line_end).collect::<Vec<_>>();
        parser.buffer.drain(..terminator_len);
        if line.len() > MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native EventSource field",
                MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES,
                line.len(),
            ));
        }
        let line = String::from_utf8(line).map_err(|_| NativeEngineError::Network {
            operation: "EventSource stream".into(),
            reason: "EventSource stream was not valid UTF-8".into(),
        })?;
        if line.is_empty() {
            if !parser.data.is_empty() {
                let data = parser
                    .data
                    .strip_suffix('\n')
                    .unwrap_or(&parser.data)
                    .to_owned();
                messages.push(NativeEventSourceMessage {
                    event: if parser.event.is_empty() {
                        "message".into()
                    } else {
                        parser.event.clone()
                    },
                    data,
                    last_event_id: parser.last_event_id.clone(),
                });
            }
            parser.data.clear();
            parser.event.clear();
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line
            .split_once(':')
            .map_or((line.as_str(), ""), |(field, value)| {
                (field, value.strip_prefix(' ').unwrap_or(value))
            });
        match field {
            "data" => {
                let next_data = parser
                    .data
                    .len()
                    .saturating_add(value.len())
                    .saturating_add(1);
                if next_data > MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native EventSource message",
                        MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES,
                        next_data,
                    ));
                }
                parser.data.push_str(value);
                parser.data.push('\n');
            }
            "event" => {
                if value.len() > MAX_NATIVE_EVENTSOURCE_FIELD_BYTES {
                    return Err(NativeEngineError::limit(
                        "native EventSource event name",
                        MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
                        value.len(),
                    ));
                }
                parser.event = value.to_owned();
            }
            "id" if !value.contains('\0') => {
                if value.len() > MAX_NATIVE_EVENTSOURCE_FIELD_BYTES {
                    return Err(NativeEngineError::limit(
                        "native EventSource last event ID",
                        MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
                        value.len(),
                    ));
                }
                parser.last_event_id = value.to_owned();
            }
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                if let Ok(milliseconds) = value.parse::<u64>() {
                    parser.retry = Duration::from_millis(
                        milliseconds.min(NATIVE_EVENTSOURCE_MAX_RETRY.as_millis() as u64),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(messages)
}

pub(super) fn bounded_websocket_text(value: impl AsRef<str>, limit: usize) -> String {
    let value = value.as_ref();
    let mut end = value.len().min(limit);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

pub fn validate_websocket_protocols(protocols: &[String]) -> Result<(), NativeEngineError> {
    if protocols.len() > MAX_NATIVE_WEBSOCKET_PROTOCOLS {
        return Err(NativeEngineError::limit(
            "native WebSocket protocols",
            MAX_NATIVE_WEBSOCKET_PROTOCOLS,
            protocols.len(),
        ));
    }
    let mut seen = BTreeSet::new();
    for protocol in protocols {
        if protocol.is_empty()
            || protocol.len() > MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES
            || !protocol.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            })
            || !seen.insert(protocol)
        {
            return Err(NativeEngineError::invalid(
                "native WebSocket protocols",
                "must contain unique bounded WebSocket tokens",
            ));
        }
    }
    Ok(())
}

pub fn native_websocket_request(
    target: &NativeWebSocketTarget,
    origin: &NativeOrigin,
    protocols: &[String],
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, NativeEngineError> {
    validate_websocket_protocols(protocols)?;
    let mut request =
        target
            .url
            .as_str()
            .into_client_request()
            .map_err(|error| NativeEngineError::Network {
                operation: "WebSocket handshake".into(),
                reason: format!("could not build request: {error}"),
            })?;
    let headers = request.headers_mut();
    headers.insert(
        tokio_tungstenite::tungstenite::http::header::ORIGIN,
        tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&origin.serialized()).map_err(
            |_| NativeEngineError::Network {
                operation: "WebSocket handshake".into(),
                reason: "document origin is not a valid request header".into(),
            },
        )?,
    );
    if let Some(cookie) = &target.cookie {
        headers.insert(
            tokio_tungstenite::tungstenite::http::header::COOKIE,
            tokio_tungstenite::tungstenite::http::HeaderValue::from_str(cookie).map_err(|_| {
                NativeEngineError::Network {
                    operation: "WebSocket handshake".into(),
                    reason: "native cookie state is not a valid request header".into(),
                }
            })?,
        );
    }
    if !protocols.is_empty() {
        let value = protocols.join(", ");
        headers.insert(
            tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL,
            tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&value).map_err(|_| {
                NativeEngineError::Network {
                    operation: "WebSocket handshake".into(),
                    reason: "WebSocket protocols are not valid request headers".into(),
                }
            })?,
        );
    }
    Ok(request)
}
