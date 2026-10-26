use super::*;

pub(super) fn content_type_charset(
    value: Option<&reqwest::header::HeaderValue>,
) -> Result<Option<String>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "HTTP content-type validation".into(),
        reason: "HTTP content type is not valid ASCII".into(),
    })?;
    let media_type = value.split(';').next().unwrap_or_default().trim();
    if media_type.eq_ignore_ascii_case("text/html")
        || media_type.eq_ignore_ascii_case("application/xhtml+xml")
    {
        let charset = value
            .split(';')
            .skip(1)
            .filter_map(|part| part.split_once('='))
            .find_map(|(name, value)| {
                name.trim()
                    .eq_ignore_ascii_case("charset")
                    .then(|| value.trim().trim_matches(['"', '\'']).to_ascii_lowercase())
            });
        return Ok(charset);
    }
    Err(NativeEngineError::UnsupportedUrl {
        reason: format!("HTTP(S) navigation returned unsupported content type {media_type:?}"),
    })
}

pub(super) fn is_http_redirect(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::MOVED_PERMANENTLY
            | reqwest::StatusCode::FOUND
            | reqwest::StatusCode::SEE_OTHER
            | reqwest::StatusCode::TEMPORARY_REDIRECT
            | reqwest::StatusCode::PERMANENT_REDIRECT
    )
}

pub(super) fn decode_html_body(
    bytes: &[u8],
    charset: Option<&str>,
    max_document_bytes: usize,
) -> Result<String, NativeEngineError> {
    let label = charset.unwrap_or_default().trim().to_ascii_lowercase();
    let decoded = if label.is_empty() {
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            String::from_utf8(bytes[3..].to_vec()).map_err(|_| ())
        } else if bytes.starts_with(&[0xff, 0xfe]) {
            decode_utf16(&bytes[2..], true)
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            decode_utf16(&bytes[2..], false)
        } else {
            String::from_utf8(bytes.to_vec()).map_err(|_| ())
        }
    } else {
        match label.as_str() {
            // HTML decoding uses the Encoding Standard's replacement behavior
            // for malformed UTF-8. A declared, supported UTF-8 charset must
            // not be reported as an unsupported encoding just because the
            // response contains a bad byte sequence.
            "utf-8" | "utf8" => Ok(String::from_utf8_lossy(bytes).into_owned()),
            "utf-16" | "utf-16le" => decode_utf16(bytes, true),
            "utf-16be" => decode_utf16(bytes, false),
            "us-ascii" => {
                if bytes.iter().any(|byte| *byte > 0x7f) {
                    return Err(charset_error(&label));
                }
                String::from_utf8(bytes.to_vec()).map_err(|_| ())
            }
            "iso-8859-1" | "latin1" | "latin-1" => {
                Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
            }
            "windows-1252" | "cp1252" => Ok(decode_windows_1252(bytes)),
            _ => return Err(charset_error(&label)),
        }
    }
    .map_err(|_| charset_error(if label.is_empty() { "utf-8" } else { &label }))?;
    if decoded.len() > max_document_bytes {
        return Err(NativeEngineError::limit(
            "decoded HTML document",
            max_document_bytes,
            decoded.len(),
        ));
    }
    Ok(decoded)
}

pub(super) fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, ()> {
    if !bytes.len().is_multiple_of(2) {
        return Err(());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| ())
}

pub(super) fn decode_windows_1252(bytes: &[u8]) -> String {
    const SPECIAL: [char; 32] = [
        '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}',
        '\u{017d}', '\u{008f}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}',
        '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|byte| {
            if (0x80..=0x9f).contains(byte) {
                SPECIAL[usize::from(*byte - 0x80)]
            } else {
                char::from(*byte)
            }
        })
        .collect()
}

pub(super) fn charset_error(charset: &str) -> NativeEngineError {
    NativeEngineError::Network {
        operation: "HTTP document decoding".into(),
        reason: format!("HTTP(S) document charset {charset:?} is unsupported or invalid"),
    }
}
