use super::*;

pub(super) fn css_comment_end(bytes: &[u8], start: usize) -> usize {
    bytes[start + 2..]
        .windows(2)
        .position(|window| window == b"*/")
        .map(|offset| start + 2 + offset + 2)
        .unwrap_or(bytes.len())
}

pub(super) fn css_quoted_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut escaped = false;
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        cursor += 1;
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            break;
        }
    }
    cursor
}

pub(super) fn css_statement_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut parentheses = 0usize;
    let mut cursor = start;
    while cursor < bytes.len() {
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            cursor = css_comment_end(bytes, cursor);
            continue;
        }
        if matches!(bytes[cursor], b'\'' | b'"') {
            cursor = css_quoted_end(bytes, cursor);
            continue;
        }
        match bytes[cursor] {
            b'(' => parentheses = parentheses.saturating_add(1),
            b')' => parentheses = parentheses.saturating_sub(1),
            b';' if parentheses == 0 => return Some(cursor),
            _ => {}
        }
        cursor += 1;
    }
    None
}

type CssImportParts = (String, Option<String>, Option<String>, Option<String>);

pub(super) fn css_import_parts(statement: &str, import_start: usize) -> Option<CssImportParts> {
    let bytes = statement.as_bytes();
    let start = css_skip_whitespace_and_comments(bytes, 0)?;
    if start >= bytes.len() {
        return None;
    }
    let (specifier, tail_start) = if matches!(bytes[start], b'\'' | b'"') {
        let quote = bytes[start];
        let end = css_quoted_end(bytes, start);
        if end <= start + 1 || bytes.get(end.saturating_sub(1)) != Some(&quote) {
            return None;
        }
        (statement[start + 1..end - 1].to_owned(), end)
    } else {
        let open = css_url_function_open(bytes, start)?;
        let close = css_url_function_close(bytes, open + 1)?;
        let mut value_start = open + 1;
        while value_start < close && bytes[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        let mut value_end = close;
        while value_end > value_start && bytes[value_end - 1].is_ascii_whitespace() {
            value_end -= 1;
        }
        if value_start == value_end {
            return None;
        }
        let token = &statement[value_start..value_end];
        let specifier = if matches!(token.as_bytes().first(), Some(b'\'' | b'"')) {
            if token.len() < 2 || token.as_bytes().last() != token.as_bytes().first() {
                return None;
            }
            token[1..token.len() - 1].to_owned()
        } else {
            token.to_owned()
        };
        (specifier, close + 1)
    };
    let mut cursor = css_skip_whitespace_and_comments(bytes, tail_start)?;
    let mut layer = None;
    if css_keyword_at(bytes, cursor, b"layer") {
        cursor = cursor.saturating_add(b"layer".len());
        let after_keyword = css_skip_whitespace_and_comments(bytes, cursor)?;
        if bytes.get(after_keyword) == Some(&b'(') {
            let close = css_parenthesized_end(bytes, after_keyword)?;
            let name_source = statement[after_keyword + 1..close].trim();
            let (name, next) = read_identifier(name_source, 0)?;
            if next != name_source.len() {
                return None;
            }
            layer = Some(name);
            cursor = close + 1;
        } else {
            layer = Some(format!("__native_import_layer_{import_start}"));
            cursor = after_keyword;
        }
    }
    cursor = css_skip_whitespace_and_comments(bytes, cursor)?;
    let mut supports = None;
    if css_keyword_at(bytes, cursor, b"supports") {
        cursor = cursor.saturating_add(b"supports".len());
        let open = css_skip_whitespace_and_comments(bytes, cursor)?;
        if bytes.get(open) != Some(&b'(') {
            return None;
        }
        let close = css_parenthesized_end(bytes, open)?;
        let condition = statement[open + 1..close].trim();
        if condition.is_empty() {
            return None;
        }
        supports = Some(condition.to_owned());
        cursor = close + 1;
    }
    cursor = css_skip_whitespace_and_comments(bytes, cursor)?;
    let media = (cursor < bytes.len()).then(|| statement[cursor..].trim().to_owned());
    Some((specifier, layer, supports, media))
}

pub(super) fn css_skip_whitespace_and_comments(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    loop {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor..cursor.saturating_add(2)) != Some(b"/*") {
            return Some(cursor);
        }
        let end = bytes[cursor + 2..]
            .windows(2)
            .position(|window| window == b"*/")?;
        cursor = cursor + 2 + end + 2;
    }
}

pub(super) fn css_parenthesized_end(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut cursor = open;
    while cursor < bytes.len() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if bytes[cursor] == b'\\' {
                escaped = true;
            } else if bytes[cursor] == delimiter {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if bytes.get(cursor..cursor.saturating_add(2)) == Some(b"/*") {
            let end = bytes[cursor + 2..]
                .windows(2)
                .position(|window| window == b"*/")?;
            cursor = cursor + 2 + end + 2;
            continue;
        }
        match bytes[cursor] {
            b'\'' | b'"' => quote = Some(bytes[cursor]),
            b'(' => depth = depth.checked_add(1)?,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            b'\\' => cursor = cursor.saturating_add(1),
            _ => {}
        }
        cursor += 1;
    }
    None
}

pub(super) fn css_keyword_at(bytes: &[u8], start: usize, keyword: &[u8]) -> bool {
    let Some(end) = start.checked_add(keyword.len()) else {
        return false;
    };
    end <= bytes.len()
        && bytes[start..end].eq_ignore_ascii_case(keyword)
        && (start == 0 || !is_css_identifier_byte(bytes[start - 1]))
        && (end == bytes.len() || !is_css_identifier_byte(bytes[end]))
}

/// Return whether a parsed import's bounded media and feature conditions are
/// active for the native viewport. Unknown media features or unsupported
/// declarations remain inactive instead of granting an import capability.
pub(crate) fn css_import_matches(import: &NativeCssImport, viewport: Viewport) -> bool {
    import
        .supports
        .as_deref()
        .is_none_or(css_supports_condition_matches)
        && import
            .media
            .as_deref()
            .is_none_or(|media| css_media_list_matches(media, viewport))
}

pub(super) fn css_media_list_matches(source: &str, viewport: Viewport) -> bool {
    let Some(queries) = css_top_level_segments(source, b',') else {
        return false;
    };
    queries
        .into_iter()
        .any(|query| css_media_query_matches(query.trim(), viewport))
}

pub(super) fn css_media_query_matches(source: &str, viewport: Viewport) -> bool {
    let Some(words) = css_top_level_words(source) else {
        return false;
    };
    if words.is_empty() {
        return false;
    }
    let mut cursor = 0usize;
    let mut invert = false;
    if words[cursor].eq_ignore_ascii_case("not") {
        invert = true;
        cursor += 1;
    } else if words[cursor].eq_ignore_ascii_case("only") {
        cursor += 1;
    }
    if cursor >= words.len() {
        return false;
    }
    let mut matches = true;
    if !words[cursor].starts_with('(') {
        matches = matches!(
            words[cursor].to_ascii_lowercase().as_str(),
            "all" | "screen"
        );
        cursor += 1;
    }
    while cursor < words.len() {
        if !words[cursor].eq_ignore_ascii_case("and") {
            return false;
        }
        cursor += 1;
        let Some(feature) = words.get(cursor) else {
            return false;
        };
        matches &= css_media_feature_matches(feature, viewport);
        cursor += 1;
    }
    if invert { !matches } else { matches }
}

pub(super) fn css_media_feature_matches(source: &str, viewport: Viewport) -> bool {
    let Some(feature) = source
        .strip_prefix('(')
        .and_then(|source| source.strip_suffix(')'))
    else {
        return false;
    };
    let Some((name, value)) = feature.split_once(':') else {
        return matches!(feature.trim().to_ascii_lowercase().as_str(), "color");
    };
    let name = name.trim().to_ascii_lowercase();
    let value = value.trim().to_ascii_lowercase();
    match name.as_str() {
        "min-width" => css_media_length(&value).is_some_and(|value| viewport.width >= value),
        "max-width" => css_media_length(&value).is_some_and(|value| viewport.width <= value),
        "min-height" => css_media_length(&value).is_some_and(|value| viewport.height >= value),
        "max-height" => css_media_length(&value).is_some_and(|value| viewport.height <= value),
        "orientation" => match value.as_str() {
            "landscape" => viewport.width >= viewport.height,
            "portrait" => viewport.width < viewport.height,
            _ => false,
        },
        "prefers-color-scheme" => value == "light",
        _ => false,
    }
}

pub(super) fn css_media_length(value: &str) -> Option<u32> {
    let value = value.strip_suffix("px").map_or(value, str::trim).trim();
    value.parse().ok()
}

pub(super) fn css_supports_condition_matches(source: &str) -> bool {
    fn evaluate(source: &str, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        let source = source.trim();
        if source.is_empty() {
            return false;
        }
        if let Some(parts) = css_split_top_level_keyword(source, "or") {
            return parts.into_iter().any(|part| evaluate(part, depth + 1));
        }
        if let Some(parts) = css_split_top_level_keyword(source, "and") {
            return parts.into_iter().all(|part| evaluate(part, depth + 1));
        }
        if source
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("not"))
            && source
                .as_bytes()
                .get(3)
                .is_some_and(u8::is_ascii_whitespace)
        {
            return !evaluate(&source[4..], depth + 1);
        }
        if let Some(inner) = css_outer_parentheses(source) {
            return evaluate(inner, depth + 1);
        }
        supports_css_declaration(source)
    }

    evaluate(source, 0)
}

pub(super) fn supports_css_declaration(source: &str) -> bool {
    let declarations = split_css_declarations(source);
    if declarations
        .iter()
        .filter(|(_, declaration)| !declaration.trim().is_empty())
        .count()
        != 1
    {
        return false;
    }
    let mut diagnostics = NativeDiagnosticSink::default();
    let _ = parse_declarations_with_diagnostics(
        source,
        NativeDiagnosticSource::Stylesheet { index: 0 },
        0,
        &mut diagnostics,
    );
    let (diagnostics, truncated) = diagnostics.finish();
    !truncated && diagnostics.is_empty()
}

pub(super) fn css_outer_parentheses(source: &str) -> Option<&str> {
    let source = source.trim();
    if !source.starts_with('(') {
        return None;
    }
    let close = css_parenthesized_end(source.as_bytes(), 0)?;
    (close == source.len().saturating_sub(1)).then(|| source[1..close].trim())
}

pub(super) fn css_split_top_level_keyword<'a>(
    source: &'a str,
    keyword: &str,
) -> Option<Vec<&'a str>> {
    let bytes = source.as_bytes();
    let keyword = keyword.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if bytes[cursor] == b'\\' {
                escaped = true;
            } else if bytes[cursor] == delimiter {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if bytes.get(cursor..cursor.saturating_add(2)) == Some(b"/*") {
            let end = bytes[cursor + 2..]
                .windows(2)
                .position(|window| window == b"*/")?;
            cursor = cursor + 2 + end + 2;
            continue;
        }
        match bytes[cursor] {
            b'\'' | b'"' => quote = Some(bytes[cursor]),
            b'(' => depth = depth.checked_add(1)?,
            b')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
        if depth == 0
            && cursor + keyword.len() <= bytes.len()
            && bytes[cursor..cursor + keyword.len()].eq_ignore_ascii_case(keyword)
            && (cursor == 0 || bytes[cursor - 1].is_ascii_whitespace())
            && (cursor + keyword.len() == bytes.len()
                || bytes[cursor + keyword.len()].is_ascii_whitespace())
        {
            let part = source[start..cursor].trim();
            if part.is_empty() {
                return None;
            }
            parts.push(part);
            cursor += keyword.len();
            start = cursor;
            continue;
        }
        if bytes[cursor] == b'\\' {
            cursor = cursor.saturating_add(1);
        } else {
            cursor += 1;
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    if parts.is_empty() {
        return None;
    }
    let part = source[start..].trim();
    (!part.is_empty()).then(|| {
        parts.push(part);
        parts
    })
}

pub(super) fn css_top_level_segments(source: &str, separator: u8) -> Option<Vec<&str>> {
    let bytes = source.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if bytes[cursor] == b'\\' {
                escaped = true;
            } else if bytes[cursor] == delimiter {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if bytes.get(cursor..cursor.saturating_add(2)) == Some(b"/*") {
            let end = bytes[cursor + 2..]
                .windows(2)
                .position(|window| window == b"*/")?;
            cursor = cursor + 2 + end + 2;
            continue;
        }
        match bytes[cursor] {
            b'\'' | b'"' => quote = Some(bytes[cursor]),
            b'(' => depth = depth.checked_add(1)?,
            b')' => depth = depth.checked_sub(1)?,
            value if value == separator && depth == 0 => {
                let segment = source[start..cursor].trim();
                if segment.is_empty() {
                    return None;
                }
                segments.push(segment);
                start = cursor + 1;
            }
            _ => {}
        }
        if bytes[cursor] == b'\\' {
            cursor = cursor.saturating_add(1);
        } else {
            cursor += 1;
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    let segment = source[start..].trim();
    if segment.is_empty() {
        return None;
    }
    segments.push(segment);
    Some(segments)
}

pub(super) fn css_top_level_words(source: &str) -> Option<Vec<&str>> {
    let bytes = source.as_bytes();
    let mut words = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if bytes[cursor] == b'\\' {
                escaped = true;
            } else if bytes[cursor] == delimiter {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if bytes.get(cursor..cursor.saturating_add(2)) == Some(b"/*") {
            let end = bytes[cursor + 2..]
                .windows(2)
                .position(|window| window == b"*/")?;
            if depth == 0 && start < cursor {
                words.push(source[start..cursor].trim());
            }
            cursor = cursor + 2 + end + 2;
            start = cursor;
            continue;
        }
        match bytes[cursor] {
            b'\'' | b'"' => quote = Some(bytes[cursor]),
            b'(' => depth = depth.checked_add(1)?,
            b')' => depth = depth.checked_sub(1)?,
            byte if byte.is_ascii_whitespace() && depth == 0 => {
                if start < cursor {
                    words.push(source[start..cursor].trim());
                }
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                start = cursor;
                continue;
            }
            _ => {}
        }
        if bytes[cursor] == b'\\' {
            cursor = cursor.saturating_add(1);
        } else {
            cursor += 1;
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    if start < bytes.len() {
        words.push(source[start..].trim());
    }
    words.retain(|word| !word.is_empty());
    Some(words)
}

pub(super) fn stylesheet_base_url(document_url: &str, stylesheet_href: &str) -> Option<Url> {
    let document_url = Url::parse(document_url).ok()?;
    let document_is_file = document_url.scheme().eq_ignore_ascii_case("file");
    let document_is_network = matches!(
        document_url.scheme().to_ascii_lowercase().as_str(),
        "http" | "https"
    );
    if !document_is_file && !document_is_network {
        return None;
    }
    if !document_url.username().is_empty() || document_url.password().is_some() {
        return None;
    }
    let mut stylesheet_url = document_url.join(stylesheet_href).ok()?;
    let stylesheet_is_file = stylesheet_url.scheme().eq_ignore_ascii_case("file");
    let stylesheet_is_network = matches!(
        stylesheet_url.scheme().to_ascii_lowercase().as_str(),
        "http" | "https"
    );
    if !stylesheet_is_file && !stylesheet_is_network {
        return None;
    }
    if document_is_file != stylesheet_is_file
        || document_is_network != stylesheet_is_network
        || !stylesheet_url.username().is_empty()
        || stylesheet_url.password().is_some()
    {
        return None;
    }
    stylesheet_url.set_fragment(None);
    Some(stylesheet_url)
}

pub(super) fn css_url_function_open(bytes: &[u8], cursor: usize) -> Option<usize> {
    css_named_function_open(bytes, cursor, b"url")
}

pub(super) fn css_named_function_open(bytes: &[u8], cursor: usize, name: &[u8]) -> Option<usize> {
    let end = cursor.checked_add(name.len())?;
    if end > bytes.len()
        || !bytes[cursor..end].eq_ignore_ascii_case(name)
        || (cursor > 0 && is_css_identifier_byte(bytes[cursor - 1]))
        || (end < bytes.len() && is_css_identifier_byte(bytes[end]))
    {
        return None;
    }
    let mut open = end;
    while open < bytes.len() && bytes[open].is_ascii_whitespace() {
        open += 1;
    }
    (bytes.get(open) == Some(&b'(')).then_some(open)
}

pub(super) fn css_url_function_close(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut offset = start;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if let Some(expected) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == expected {
                quote = None;
            }
            offset += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b')' => return Some(offset),
            b'\\' => {
                offset = offset.saturating_add(2);
                continue;
            }
            _ => {}
        }
        offset += 1;
    }
    None
}

pub(super) fn is_css_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

/// Decode CSS escape sequences inside a URL token before handing the value to
/// the URL parser. This is intentionally scoped to URL-like tokens: selector,
/// declaration, and string parsing retain their existing owners. Invalid
/// trailing escapes are rejected so a file capability cannot resolve a
/// partially interpreted path.
pub(crate) fn decode_css_url_value(value: &str) -> Option<String> {
    let mut decoded = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let next = chars.next()?;
        if next == '\n' {
            continue;
        }
        if next == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            continue;
        }
        if next.is_ascii_hexdigit() {
            let mut digits = String::with_capacity(6);
            digits.push(next);
            while digits.len() < 6
                && chars
                    .peek()
                    .is_some_and(|candidate| candidate.is_ascii_hexdigit())
            {
                digits.push(chars.next().expect("peeked CSS hex escape"));
            }
            if chars.peek().is_some_and(char::is_ascii_whitespace) {
                chars.next();
            }
            let codepoint = u32::from_str_radix(&digits, 16).ok()?;
            decoded.push(
                (codepoint != 0)
                    .then(|| char::from_u32(codepoint))
                    .flatten()
                    .unwrap_or('\u{fffd}'),
            );
        } else {
            decoded.push(next);
        }
    }
    Some(decoded)
}

pub(super) fn resolve_css_url_token(token: &str, base_url: &Url) -> Option<String> {
    let (quoted, value) = if token.len() >= 2
        && matches!(token.as_bytes().first(), Some(b'\'' | b'"'))
        && token.as_bytes().last() == token.as_bytes().first()
    {
        (
            Some(token.as_bytes()[0] as char),
            &token[1..token.len() - 1],
        )
    } else {
        (None, token)
    };
    let value = decode_css_url_value(value)?;
    if value.is_empty() || value.starts_with('#') || value.bytes().any(|byte| byte == 0) {
        return None;
    }
    let target = Url::parse(&value).or_else(|_| base_url.join(&value)).ok()?;
    let base_is_file = base_url.scheme().eq_ignore_ascii_case("file");
    let target_is_file = target.scheme().eq_ignore_ascii_case("file");
    let target_is_network = matches!(
        target.scheme().to_ascii_lowercase().as_str(),
        "http" | "https"
    );
    if (base_is_file != target_is_file)
        || (!base_is_file && !target_is_network)
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return None;
    }
    let target = target.to_string();
    quoted.map_or_else(
        || Some(target.clone()),
        |quote| Some(format!("{quote}{target}{quote}")),
    )
}

pub(super) fn register_background_image_source(
    source: &str,
    background_image_sources: &mut BTreeMap<u32, String>,
) {
    let source_id = background_image_source_id(source);
    if background_image_sources
        .get(&source_id)
        .is_none_or(|existing| existing == source)
    {
        background_image_sources
            .entry(source_id)
            .or_insert_with(|| source.to_owned());
    }
}

pub(super) fn split_css_value_tokens(value: &str) -> Option<Vec<&str>> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut parentheses = 0usize;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => {
                if start.is_none() {
                    start = Some(index);
                }
                parentheses = parentheses.checked_add(1)?;
            }
            b')' => {
                parentheses = parentheses.checked_sub(1)?;
            }
            byte if byte.is_ascii_whitespace() && parentheses == 0 => {
                if let Some(start) = start.take() {
                    tokens.push(&value[start..index]);
                }
            }
            _ if start.is_none() => start = Some(index),
            _ => {}
        }
    }
    if parentheses != 0 {
        return None;
    }
    if let Some(start) = start {
        tokens.push(&value[start..]);
    }
    (!tokens.is_empty()).then_some(tokens)
}
