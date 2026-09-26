//! Bounded WHATWG-compatible encoding selection for HTML XHR documents.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252, X_USER_DEFINED};

const HTML_PRESCAN_BYTES: usize = 1024;

pub(super) fn decode_xhr_html_document(
    bytes: &[u8],
    response_content_type: Option<&str>,
    override_mime_type: Option<&str>,
) -> String {
    let response_label = mime_charset_parameter(response_content_type);
    let override_label = mime_charset_parameter(override_mime_type);
    let final_label = override_label.or(response_label);
    let encoding = final_label
        .as_deref()
        .and_then(encoding_for_label)
        .or_else(|| prescan_html_encoding(bytes))
        .unwrap_or(UTF_8);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

fn mime_charset_parameter(mime_type: Option<&str>) -> Option<Vec<u8>> {
    let bytes = mime_type?.as_bytes();
    let mut position = bytes.iter().position(|byte| *byte == b';')? + 1;

    while position < bytes.len() {
        while position < bytes.len() && (is_ascii_space(bytes[position]) || bytes[position] == b';')
        {
            position += 1;
        }
        if position == bytes.len() {
            break;
        }

        let start = position;
        let mut quote = None;
        let mut escaped = false;
        while position < bytes.len() {
            let byte = bytes[position];
            if let Some(quote_byte) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' && quote_byte == b'"' {
                    escaped = true;
                } else if byte == quote_byte {
                    quote = None;
                }
            } else if byte == b'"' || byte == b'\'' {
                quote = Some(byte);
            } else if byte == b';' {
                break;
            }
            position += 1;
        }

        let parameter = &bytes[start..position];
        if let Some(separator) = parameter.iter().position(|byte| *byte == b'=') {
            if trim_ascii_whitespace(&parameter[..separator]).eq_ignore_ascii_case(b"charset") {
                let mut value = trim_ascii_whitespace(&parameter[separator + 1..]);
                if value.len() >= 2
                    && matches!(value[0], b'"' | b'\'')
                    && value.last() == value.first()
                {
                    value = &value[1..value.len() - 1];
                }
                return Some(value.to_vec());
            }
        }

        if position < bytes.len() {
            position += 1;
        }
    }
    None
}

fn encoding_for_label(label: &[u8]) -> Option<&'static Encoding> {
    Encoding::for_label(trim_ascii_whitespace(label))
}

fn prescan_html_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let input = &bytes[..bytes.len().min(HTML_PRESCAN_BYTES)];
    if input.starts_with(b"<\0?\0x\0") {
        return Some(UTF_16LE);
    }
    if input.starts_with(b"\0<\0?\0x") {
        return Some(UTF_16BE);
    }

    let mut position = 0;
    while position < input.len() {
        let remaining = &input[position..];
        if remaining.starts_with(b"<!--") {
            let Some(comment_end) = find_subsequence(remaining, b"-->") else {
                break;
            };
            position += comment_end + 3;
            continue;
        }

        if remaining.len() >= 6
            && remaining[..5].eq_ignore_ascii_case(b"<meta")
            && is_meta_delimiter(remaining[5])
        {
            let mut attribute_position = position + 5;
            let attributes = prescan_tag_attributes(input, &mut attribute_position);
            if let Some(encoding) = meta_encoding(&attributes) {
                return Some(adjust_meta_encoding(encoding));
            }
            if input.get(attribute_position) == Some(&b'>') {
                position = attribute_position + 1;
                continue;
            }
            break;
        }

        if remaining.first() == Some(&b'<') {
            let mut tag_name_position = position + 1;
            if input.get(tag_name_position) == Some(&b'/') {
                tag_name_position += 1;
            }
            if input
                .get(tag_name_position)
                .is_some_and(u8::is_ascii_alphabetic)
            {
                let mut attribute_position = tag_name_position;
                while attribute_position < input.len()
                    && !is_ascii_whitespace(input[attribute_position])
                    && input[attribute_position] != b'>'
                {
                    attribute_position += 1;
                }
                let _ = prescan_tag_attributes(input, &mut attribute_position);
                if input.get(attribute_position) == Some(&b'>') {
                    position = attribute_position + 1;
                    continue;
                }
                break;
            }
            if matches!(
                input.get(position + 1),
                Some(b'!') | Some(b'/') | Some(b'?')
            ) {
                let Some(tag_end) = input[position + 1..].iter().position(|byte| *byte == b'>')
                else {
                    break;
                };
                position += tag_end + 2;
                continue;
            }
        }
        position += 1;
    }

    xml_declaration_encoding(input)
}

fn is_meta_delimiter(byte: u8) -> bool {
    is_ascii_whitespace(byte) || byte == b'/'
}

fn prescan_tag_attributes(input: &[u8], position: &mut usize) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut attributes = Vec::new();
    loop {
        let Some(attribute) = prescan_next_attribute(input, position) else {
            break;
        };
        attributes.push(attribute);
    }
    attributes
}

fn prescan_next_attribute(input: &[u8], position: &mut usize) -> Option<(Vec<u8>, Vec<u8>)> {
    while input
        .get(*position)
        .is_some_and(|byte| is_ascii_whitespace(*byte) || *byte == b'/')
    {
        *position += 1;
    }
    if *position >= input.len() || input[*position] == b'>' {
        return None;
    }

    let mut name = Vec::new();
    loop {
        let Some(&byte) = input.get(*position) else {
            return None;
        };
        match byte {
            b'=' if !name.is_empty() => {
                *position += 1;
                return prescan_attribute_value(input, position).map(|value| (name, value));
            }
            b'=' => name.push(byte),
            byte if is_ascii_whitespace(byte) => {
                while input
                    .get(*position)
                    .is_some_and(|byte| is_ascii_whitespace(*byte))
                {
                    *position += 1;
                }
                if input.get(*position) == Some(&b'=') {
                    *position += 1;
                    return prescan_attribute_value(input, position).map(|value| (name, value));
                }
                return Some((name, Vec::new()));
            }
            b'/' | b'>' => return Some((name, Vec::new())),
            _ => name.push(byte.to_ascii_lowercase()),
        }
        *position += 1;
    }
}

fn prescan_attribute_value(input: &[u8], position: &mut usize) -> Option<Vec<u8>> {
    while input
        .get(*position)
        .is_some_and(|byte| is_ascii_whitespace(*byte))
    {
        *position += 1;
    }
    let first = *input.get(*position)?;
    if first == b'>' {
        return Some(Vec::new());
    }
    if first == b'"' || first == b'\'' {
        *position += 1;
        let start = *position;
        while let Some(&byte) = input.get(*position) {
            if byte == first {
                let value = input[start..*position]
                    .iter()
                    .map(u8::to_ascii_lowercase)
                    .collect();
                *position += 1;
                return Some(value);
            }
            *position += 1;
        }
        return None;
    }

    let start = *position;
    while input
        .get(*position)
        .is_some_and(|byte| !is_ascii_whitespace(*byte) && *byte != b'>')
    {
        *position += 1;
    }
    Some(
        input[start..*position]
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
}

fn meta_encoding(attributes: &[(Vec<u8>, Vec<u8>)]) -> Option<&'static Encoding> {
    let mut seen = Vec::<&[u8]>::new();
    let mut got_pragma = false;
    let mut charset: Option<Option<&'static Encoding>> = None;
    let mut need_pragma = None;

    for (name, value) in attributes {
        if seen.iter().any(|previous| *previous == name.as_slice()) {
            continue;
        }
        seen.push(name.as_slice());
        match name.as_slice() {
            b"http-equiv" if value.eq_ignore_ascii_case(b"content-type") => {
                got_pragma = true;
            }
            b"content" => {
                if charset.is_none()
                    && let Some(encoding) = extract_meta_content_encoding(value)
                {
                    charset = Some(Some(encoding));
                    need_pragma = Some(true);
                }
            }
            b"charset" => {
                charset = Some(encoding_for_label(value));
                need_pragma = Some(false);
            }
            _ => {}
        }
    }

    if need_pragma == Some(true) && !got_pragma {
        return None;
    }
    charset.flatten()
}

fn extract_meta_content_encoding(content: &[u8]) -> Option<&'static Encoding> {
    let mut position = 0;
    while position + b"charset".len() <= content.len() {
        let relative = content[position..]
            .windows(b"charset".len())
            .position(|candidate| candidate.eq_ignore_ascii_case(b"charset"))?;
        position += relative + b"charset".len();
        let mut value_position = position;
        while content
            .get(value_position)
            .is_some_and(|byte| is_ascii_whitespace(*byte))
        {
            value_position += 1;
        }
        if content.get(value_position) != Some(&b'=') {
            position = value_position;
            continue;
        }
        value_position += 1;
        while content
            .get(value_position)
            .is_some_and(|byte| is_ascii_whitespace(*byte))
        {
            value_position += 1;
        }
        let first = *content.get(value_position)?;
        let label = if first == b'"' || first == b'\'' {
            value_position += 1;
            let end = content[value_position..]
                .iter()
                .position(|byte| *byte == first)?
                + value_position;
            &content[value_position..end]
        } else {
            let end = content[value_position..]
                .iter()
                .position(|byte| is_ascii_whitespace(*byte) || *byte == b';')
                .map_or(content.len(), |relative| value_position + relative);
            &content[value_position..end]
        };
        return encoding_for_label(label);
    }
    None
}

fn adjust_meta_encoding(encoding: &'static Encoding) -> &'static Encoding {
    if encoding == UTF_16LE || encoding == UTF_16BE {
        UTF_8
    } else if encoding == X_USER_DEFINED {
        WINDOWS_1252
    } else {
        encoding
    }
}

fn xml_declaration_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    if !bytes.starts_with(b"<?xml") {
        return None;
    }
    let declaration_end = bytes.iter().position(|byte| *byte == b'>')?;
    let declaration = &bytes[..declaration_end];
    let name_position = find_subsequence(declaration, b"encoding")? + b"encoding".len();
    let mut position = name_position;
    while bytes.get(position).is_some_and(|byte| *byte <= 0x20) {
        position += 1;
    }
    if bytes.get(position) != Some(&b'=') {
        return None;
    }
    position += 1;
    while bytes.get(position).is_some_and(|byte| *byte <= 0x20) {
        position += 1;
    }
    let quote = *bytes.get(position)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    position += 1;
    let end = bytes[position..declaration_end]
        .iter()
        .position(|byte| *byte == quote)?
        + position;
    let label = &bytes[position..end];
    if label.iter().any(|byte| *byte <= 0x20) {
        return None;
    }
    let encoding = encoding_for_label(label)?;
    Some(if encoding == UTF_16LE || encoding == UTF_16BE {
        UTF_8
    } else {
        encoding
    })
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn is_ascii_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b' ')
}

fn trim_ascii_whitespace(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(|byte| is_ascii_whitespace(*byte)) {
        value = &value[1..];
    }
    while value.last().is_some_and(|byte| is_ascii_whitespace(*byte)) {
        value = &value[..value.len() - 1];
    }
    value
}

#[cfg(test)]
mod tests {
    use super::decode_xhr_html_document;

    #[test]
    fn response_charset_uses_the_whatwg_label_table_and_beats_meta() {
        let bytes = b"<meta charset=shift_jis><title>caf\xe9</title>";
        let decoded = decode_xhr_html_document(bytes, Some("text/html; charset=latin1"), None);
        assert!(decoded.contains("caf\u{e9}"));
        assert!(!decoded.contains("日本"));
    }

    #[test]
    fn invalid_override_falls_through_to_direct_meta_charset() {
        let bytes = b"<meta charset=Shift_JIS><title>\x93\xfa\x96\x7b\x8c\xea</title>";
        let decoded = decode_xhr_html_document(
            bytes,
            Some("text/html; charset=windows-1252"),
            Some("text/html; charset=unknown-native-label"),
        );
        assert!(decoded.contains("日本語"));
    }

    #[test]
    fn meta_content_requires_pragma_and_supports_gbk() {
        let bytes = b"<meta http-equiv='Content-Type' content=\"text/html; charset=gbk\"><title>\xd6\xd0\xce\xc4</title>";
        let decoded = decode_xhr_html_document(bytes, Some("text/html; charset=bad-label"), None);
        assert!(decoded.contains("中文"));

        let without_pragma =
            b"<meta content=\"text/html; charset=gbk\"><title>\xd6\xd0\xce\xc4</title>";
        let decoded = decode_xhr_html_document(without_pragma, None, None);
        assert!(!decoded.contains("中文"));
    }

    #[test]
    fn comments_and_duplicate_attributes_do_not_supply_a_later_encoding() {
        let bytes = b"<!--<meta charset=shift_jis>--><meta charset=bad-label charset=shift_jis><meta charset=gbk><title>\xd6\xd0\xce\xc4</title>";
        let decoded = decode_xhr_html_document(bytes, None, None);
        assert!(decoded.contains("中文"));
        assert!(!decoded.contains("日本"));
    }

    #[test]
    fn prescan_stops_at_1024_bytes() {
        let mut bytes = vec![b' '; 1024];
        bytes.extend_from_slice(b"<meta charset=shift_jis><title>\x93\xfa\x96\x7b</title>");
        let decoded = decode_xhr_html_document(&bytes, None, None);
        assert!(!decoded.contains("日本"));
    }

    #[test]
    fn xml_declaration_fallback_and_utf16_xml_prefix_are_bounded() {
        let declared = b"<?xml version='1.0' encoding='windows-1252'?><title>caf\xe9</title>";
        assert!(decode_xhr_html_document(declared, None, None).contains("caf\u{e9}"));

        let mut utf16 = b"<\0?\0x\0m\0l\0?><\0t\0i\0t\0l\0e\0>\0".to_vec();
        utf16.extend("é</title>".encode_utf16().flat_map(u16::to_le_bytes));
        assert!(decode_xhr_html_document(&utf16, None, None).contains("é"));
    }

    #[test]
    fn html_meta_normalizes_utf16_and_x_user_defined_labels() {
        let utf16_label = b"<meta charset=utf-16le><title>caf\xc3\xa9</title>";
        assert!(decode_xhr_html_document(utf16_label, None, None).contains("café"));

        let user_defined = b"<meta charset=x-user-defined><title>\x80</title>";
        assert!(decode_xhr_html_document(user_defined, None, None).contains("€"));
    }

    #[test]
    fn malformed_sequences_are_replaced() {
        let decoded =
            decode_xhr_html_document(b"<p>\xf0(\x8c(</p>", Some("text/html; charset=utf-8"), None);
        assert!(decoded.contains('\u{fffd}'));
    }

    #[test]
    fn mime_parameter_parser_keeps_semicolons_inside_other_quoted_parameters() {
        let bytes = b"<title>caf\xe9</title>";
        let decoded = decode_xhr_html_document(
            bytes,
            Some("text/html; note=\"not;charset=shift_jis\"; charset='windows-1252'"),
            None,
        );
        assert!(decoded.contains("café"));
    }
}
