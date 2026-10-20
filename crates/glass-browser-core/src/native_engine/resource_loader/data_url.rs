use super::*;

pub(super) fn decode_base64_bytes(
    payload: &str,
    max_bytes: usize,
    resource_name: &'static str,
) -> Result<Vec<u8>, NativeEngineError> {
    let max_encoded_bytes = (max_bytes.saturating_add(2) / 3).saturating_mul(4);
    if payload.len() > max_encoded_bytes {
        return Err(NativeEngineError::limit(
            resource_name,
            max_encoded_bytes,
            payload.len(),
        ));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "data URL contains an invalid standard base64 payload".into(),
        })?;
    if decoded.len() > max_bytes {
        return Err(NativeEngineError::limit(
            resource_name,
            max_bytes,
            decoded.len(),
        ));
    }
    Ok(decoded)
}

pub(super) fn decode_base64(
    payload: &str,
    max_document_bytes: usize,
) -> Result<String, NativeEngineError> {
    let decoded = decode_base64_bytes(payload, max_document_bytes, "base64 data payload")?;
    String::from_utf8(decoded).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "base64 data URL payload is not valid UTF-8 HTML".into(),
    })
}

pub(super) fn percent_decode(value: &str) -> Result<String, NativeEngineError> {
    let decoded = percent_decode_bytes(value, usize::MAX)?;
    String::from_utf8(decoded).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "data URL payload is not valid UTF-8 HTML".into(),
    })
}

pub(super) fn percent_decode_bytes(
    value: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, NativeEngineError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len().min(max_bytes));
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            if decoded.len() > max_bytes {
                return Err(NativeEngineError::limit(
                    "percent-encoded media payload",
                    max_bytes,
                    decoded.len(),
                ));
            }
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL contains an incomplete percent escape".into(),
            });
        }
        let Some(high) = hex_value(bytes[index + 1]) else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL contains an invalid percent escape".into(),
            });
        };
        let Some(low) = hex_value(bytes[index + 2]) else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL contains an invalid percent escape".into(),
            });
        };
        decoded.push((high << 4) | low);
        if decoded.len() > max_bytes {
            return Err(NativeEngineError::limit(
                "percent-encoded media payload",
                max_bytes,
                decoded.len(),
            ));
        }
        index += 3;
    }
    Ok(decoded)
}

pub(super) fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
