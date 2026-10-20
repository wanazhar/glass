use super::*;
use std::borrow::Cow;

#[derive(Debug)]
pub(super) enum HtmlToken {
    StartTag {
        name: String,
        attributes: BTreeMap<String, String>,
        self_closing: bool,
    },
    EndTag(String),
    Comment(String),
    #[cfg_attr(not(test), allow(dead_code))]
    Doctype {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    Text(String),
    RawText(String),
    RcData(String),
}

pub(super) fn tokenize(
    source: &str,
    max_tokens: usize,
) -> Result<Vec<HtmlToken>, NativeEngineError> {
    let normalized_source = normalize_html_newlines(source);
    tokenize_preprocessed_html(normalized_source.as_ref(), max_tokens).map_err(
        |error| match error {
            NativeEngineError::Parse { offset, reason } => NativeEngineError::Parse {
                offset: original_html_source_offset(source, offset),
                reason,
            },
            other => other,
        },
    )
}

fn normalize_html_newlines(source: &str) -> Cow<'_, str> {
    if !source.as_bytes().contains(&b'\r') {
        return Cow::Borrowed(source);
    }

    let mut normalized = String::with_capacity(source.len());
    let mut copied_through = 0;
    let mut search_from = 0;
    while let Some(relative_cr) = source[search_from..].find('\r') {
        let cr = search_from + relative_cr;
        normalized.push_str(&source[copied_through..cr]);
        normalized.push('\n');
        search_from = cr + 1;
        if source.as_bytes().get(search_from) == Some(&b'\n') {
            search_from += 1;
        }
        copied_through = search_from;
    }
    normalized.push_str(&source[copied_through..]);
    Cow::Owned(normalized)
}

fn original_html_source_offset(source: &str, normalized_offset: usize) -> usize {
    let mut source_offset = 0;
    let mut normalized_offset_seen = 0;
    let bytes = source.as_bytes();
    while source_offset < source.len() && normalized_offset_seen < normalized_offset {
        if bytes[source_offset] == b'\r' {
            source_offset += 1;
            if bytes.get(source_offset) == Some(&b'\n') {
                source_offset += 1;
            }
            normalized_offset_seen += 1;
        } else {
            let character = source[source_offset..]
                .chars()
                .next()
                .expect("source offset is before the end of a valid UTF-8 string");
            let width = character.len_utf8();
            source_offset += width;
            normalized_offset_seen += width;
        }
    }
    source_offset
}

fn tokenize_preprocessed_html(
    source: &str,
    max_tokens: usize,
) -> Result<Vec<HtmlToken>, NativeEngineError> {
    let mut tokens = Vec::new();
    let mut position = 0;
    while position < source.len() {
        if source.as_bytes()[position] != b'<' {
            let end = source[position..]
                .find('<')
                .map_or(source.len(), |relative| position + relative);
            push_token(
                &mut tokens,
                HtmlToken::Text(source[position..end].into()),
                max_tokens,
            )?;
            position = end;
            continue;
        }
        if source[position..].starts_with("<!--") {
            let relative_end = source[position + 4..].find("-->");
            let comment_end =
                relative_end.unwrap_or_else(|| source.len().saturating_sub(position + 4));
            push_token(
                &mut tokens,
                HtmlToken::Comment(
                    replace_html_nulls(
                        &source[position + 4..position + 4 + comment_end],
                        "\u{FFFD}",
                    )
                    .into_owned(),
                ),
                max_tokens,
            )?;
            position = relative_end.map_or(source.len(), |end| position + 4 + end + 3);
            continue;
        }
        let doctype_prefix = position.saturating_add(9);
        if source
            .as_bytes()
            .get(position..doctype_prefix)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"<!doctype"))
            && source
                .as_bytes()
                .get(doctype_prefix)
                .is_none_or(|byte| byte.is_ascii_whitespace() || *byte == b'>')
        {
            let end = find_tag_end(source, doctype_prefix).unwrap_or(source.len());
            if let Some((name, public_id, system_id)) = parse_doctype(&source[doctype_prefix..end])
            {
                push_token(
                    &mut tokens,
                    HtmlToken::Doctype {
                        name,
                        public_id,
                        system_id,
                    },
                    max_tokens,
                )?;
            }
            position = if end < source.len() {
                end + 1
            } else {
                source.len()
            };
            continue;
        }
        if source[position..].starts_with("<!") || source[position..].starts_with("<?") {
            // HTML treats unknown declarations and processing-instruction
            // syntax as bogus comments. Recover to EOF when the declaration
            // is incomplete instead of discarding the remainder or failing a
            // whole navigation.
            let end = find_tag_end(source, position + 2);
            let comment_end = end.unwrap_or(source.len());
            push_token(
                &mut tokens,
                HtmlToken::Comment(
                    replace_html_nulls(&source[position + 2..comment_end], "\u{FFFD}").into_owned(),
                ),
                max_tokens,
            )?;
            position = end.map_or(source.len(), |value| value + 1);
            continue;
        }
        let is_end_tag = source[position..].starts_with("</");
        let name_position = position + if is_end_tag { 2 } else { 1 };
        if name_position >= source.len() || !is_tag_name_start(source.as_bytes()[name_position]) {
            push_token(&mut tokens, HtmlToken::Text("<".into()), max_tokens)?;
            position += 1;
            continue;
        }
        let Some(end) = find_tag_end(source, name_position) else {
            // EOF in a tag is a parse error, but it is still recoverable HTML
            // input. Preserve the source as text so a malformed response does
            // not turn into a synthetic partial element.
            push_token(
                &mut tokens,
                HtmlToken::Text(source[position..].to_owned()),
                max_tokens,
            )?;
            break;
        };
        let mut next_position = end + 1;
        if is_end_tag {
            let raw = &source[name_position..end];
            let Some((name, _)) = read_name(raw, 0) else {
                return Err(NativeEngineError::Parse {
                    offset: position,
                    reason: "end tag has no element name".into(),
                });
            };
            push_token(&mut tokens, HtmlToken::EndTag(name), max_tokens)?;
        } else {
            let raw = &source[name_position..end];
            let (name, attributes, self_closing) = parse_start_tag(raw, position)?;
            let special_text_mode = special_text_mode(&name);
            push_token(
                &mut tokens,
                HtmlToken::StartTag {
                    name: name.clone(),
                    attributes,
                    self_closing,
                },
                max_tokens,
            )?;
            if name == "frame" && !self_closing {
                let text_start = end + 1;
                if let Some(text_end) = find_raw_text_end(source, text_start, &name) {
                    next_position = text_end;
                } else {
                    break;
                }
            }
            if let Some(decode_entities) = special_text_mode
                && !self_closing
                && !is_void_element(&name)
            {
                let text_start = end + 1;
                let Some(text_end) = find_raw_text_end(source, text_start, &name) else {
                    let value = source[text_start..].to_owned();
                    push_token(
                        &mut tokens,
                        if decode_entities {
                            HtmlToken::RcData(replace_html_nulls(&value, "\u{FFFD}").into_owned())
                        } else {
                            HtmlToken::RawText(replace_html_nulls(&value, "\u{FFFD}").into_owned())
                        },
                        max_tokens,
                    )?;
                    break;
                };
                let value = source[text_start..text_end].to_owned();
                push_token(
                    &mut tokens,
                    if decode_entities {
                        HtmlToken::RcData(replace_html_nulls(&value, "\u{FFFD}").into_owned())
                    } else {
                        HtmlToken::RawText(replace_html_nulls(&value, "\u{FFFD}").into_owned())
                    },
                    max_tokens,
                )?;
                next_position = text_end;
            }
        }
        position = next_position;
    }
    Ok(tokens)
}

fn push_token(
    tokens: &mut Vec<HtmlToken>,
    token: HtmlToken,
    max_tokens: usize,
) -> Result<(), NativeEngineError> {
    if tokens.len() >= max_tokens {
        return Err(NativeEngineError::limit(
            "HTML tokens",
            max_tokens,
            tokens.len().saturating_add(1),
        ));
    }
    if matches!(&token, HtmlToken::Text(value) | HtmlToken::RawText(value) | HtmlToken::RcData(value) if value.is_empty())
    {
        return Ok(());
    }
    tokens.push(token);
    Ok(())
}

fn find_tag_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote = None;
    for (offset, byte) in bytes[start..].iter().copied().enumerate() {
        match quote {
            Some(active) if byte == active => quote = None,
            None if byte == b'\'' || byte == b'"' => quote = Some(byte),
            None if byte == b'>' => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn parse_start_tag(
    raw: &str,
    offset: usize,
) -> Result<(String, BTreeMap<String, String>, bool), NativeEngineError> {
    let (name, mut cursor) = read_name(raw, 0).ok_or_else(|| NativeEngineError::Parse {
        offset,
        reason: "start tag has no element name".into(),
    })?;
    let mut attributes = BTreeMap::new();
    let mut self_closing = false;
    let bytes = raw.as_bytes();
    while cursor < raw.len() {
        skip_ascii_whitespace(bytes, &mut cursor);
        if cursor >= raw.len() {
            break;
        }
        if bytes[cursor] == b'/' {
            self_closing = true;
            cursor += 1;
            continue;
        }
        let Some((attribute_name, next)) = read_name(raw, cursor) else {
            cursor += 1;
            continue;
        };
        cursor = next;
        skip_ascii_whitespace(bytes, &mut cursor);
        let mut value = String::new();
        if cursor < raw.len() && bytes[cursor] == b'=' {
            cursor += 1;
            skip_ascii_whitespace(bytes, &mut cursor);
            if cursor >= raw.len() {
                return Err(NativeEngineError::Parse {
                    offset,
                    reason: "attribute is missing its value".into(),
                });
            }
            if bytes[cursor] == b'\'' || bytes[cursor] == b'"' {
                let quote = bytes[cursor];
                cursor += 1;
                let value_start = cursor;
                while cursor < raw.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                if cursor >= raw.len() {
                    return Err(NativeEngineError::Parse {
                        offset,
                        reason: "quoted attribute is unterminated".into(),
                    });
                }
                value.push_str(&raw[value_start..cursor]);
                cursor += 1;
            } else {
                let value_start = cursor;
                while cursor < raw.len() && !bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                value.push_str(&raw[value_start..cursor]);
            }
        }
        if attribute_name.len() > MAX_ATTRIBUTE_BYTES || value.len() > MAX_ATTRIBUTE_BYTES {
            return Err(NativeEngineError::limit(
                "HTML attribute",
                MAX_ATTRIBUTE_BYTES,
                attribute_name.len().max(value.len()),
            ));
        }
        // The HTML tokenizer keeps the first attribute with a given
        // ASCII-case-insensitive name and ignores later duplicates.
        attributes
            .entry(attribute_name)
            .or_insert_with(|| decode_html_attribute_value(&value));
    }
    Ok((name, attributes, self_closing))
}

fn parse_doctype(raw: &str) -> Option<(String, Option<String>, Option<String>)> {
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    skip_ascii_whitespace(bytes, &mut cursor);
    let (name, next) = read_name(raw, cursor)?;
    cursor = next;
    skip_ascii_whitespace(bytes, &mut cursor);
    if cursor >= raw.len() {
        return Some((name, None, None));
    }

    let (keyword, next) = read_name(raw, cursor)?;
    cursor = next;
    skip_ascii_whitespace(bytes, &mut cursor);
    if keyword.eq_ignore_ascii_case("public") {
        let (public_id, next) = read_quoted_value(raw, cursor)?;
        cursor = next;
        skip_ascii_whitespace(bytes, &mut cursor);
        let system_id = read_quoted_value(raw, cursor).map(|(value, _)| value);
        return Some((name, Some(public_id), system_id));
    }
    if keyword.eq_ignore_ascii_case("system") {
        let (system_id, _) = read_quoted_value(raw, cursor)?;
        return Some((name, None, Some(system_id)));
    }
    Some((name, None, None))
}

fn read_quoted_value(raw: &str, start: usize) -> Option<(String, usize)> {
    let bytes = raw.as_bytes();
    let quote = *bytes.get(start)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let value_start = start + 1;
    let relative_end = raw[value_start..].find(quote as char)?;
    let end = value_start + relative_end;
    Some((raw[value_start..end].to_owned(), end + 1))
}

pub(super) fn read_name(raw: &str, start: usize) -> Option<(String, usize)> {
    let bytes = raw.as_bytes();
    if start >= bytes.len() || !is_tag_name_start(bytes[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && is_tag_name_char(bytes[end]) {
        end += 1;
    }
    Some((raw[start..end].to_ascii_lowercase(), end))
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn is_tag_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_tag_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':')
}

fn special_text_mode(name: &str) -> Option<bool> {
    if is_html_raw_text_element(name) {
        Some(false)
    } else {
        match name {
            "title" | "textarea" => Some(true),
            _ => None,
        }
    }
}

fn is_html_raw_text_element(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript"
    )
}

#[derive(Clone, Copy)]
enum HtmlScriptDataState {
    Data,
    LessThan,
    EndTagOpen,
    EndTagName,
    EscapeStart,
    EscapeStartDash,
    Escaped,
    EscapedDash,
    EscapedDashDash,
    EscapedLessThan,
    EscapedEndTagOpen,
    EscapedEndTagName,
    DoubleEscapeStart,
    DoubleEscaped,
    DoubleEscapedDash,
    DoubleEscapedDashDash,
    DoubleEscapedLessThan,
    DoubleEscapeEnd,
}

fn find_raw_text_end(source: &str, start: usize, name: &str) -> Option<usize> {
    if name == "script" {
        return find_script_data_end(source, start);
    }

    let bytes = source.as_bytes();
    let mut candidate = start;
    while candidate < bytes.len() {
        let relative = source[candidate..].find('<')?;
        let opening = candidate + relative;
        let name_start = opening.checked_add(2)?;
        let name_end = name_start.checked_add(name.len())?;
        if bytes.get(opening + 1) == Some(&b'/')
            && name_end <= bytes.len()
            && bytes[name_start..name_end].eq_ignore_ascii_case(name.as_bytes())
            && bytes
                .get(name_end)
                .is_none_or(|byte| byte.is_ascii_whitespace() || matches!(*byte, b'/' | b'>'))
        {
            return Some(opening);
        }
        candidate = opening.saturating_add(1);
    }
    None
}

pub(super) fn find_script_data_end(source: &str, start: usize) -> Option<usize> {
    fn is_delimiter(byte: u8) -> bool {
        matches!(byte, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' | b'/' | b'>')
    }

    let bytes = source.as_bytes();
    let mut state = HtmlScriptDataState::Data;
    let mut position = start;
    let mut tag_start = 0;
    let mut temporary_buffer = Vec::new();

    while let Some(&byte) = bytes.get(position) {
        let mut reconsume = false;
        match state {
            HtmlScriptDataState::Data => {
                if byte == b'<' {
                    state = HtmlScriptDataState::LessThan;
                }
            }
            HtmlScriptDataState::LessThan => match byte {
                b'/' => {
                    tag_start = position.saturating_sub(1);
                    temporary_buffer.clear();
                    state = HtmlScriptDataState::EndTagOpen;
                }
                b'!' => state = HtmlScriptDataState::EscapeStart,
                _ => {
                    state = HtmlScriptDataState::Data;
                    reconsume = true;
                }
            },
            HtmlScriptDataState::EndTagOpen => {
                if byte.is_ascii_alphabetic() {
                    state = HtmlScriptDataState::EndTagName;
                    reconsume = true;
                } else {
                    state = HtmlScriptDataState::Data;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::EndTagName => {
                if byte.is_ascii_alphabetic() {
                    temporary_buffer.push(byte.to_ascii_lowercase());
                } else if is_delimiter(byte) {
                    if temporary_buffer == b"script" {
                        return find_tag_end(source, position).map(|_| tag_start);
                    }
                    state = HtmlScriptDataState::Data;
                    reconsume = true;
                } else {
                    state = HtmlScriptDataState::Data;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::EscapeStart => {
                if byte == b'-' {
                    state = HtmlScriptDataState::EscapeStartDash;
                } else {
                    state = HtmlScriptDataState::Data;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::EscapeStartDash => {
                if byte == b'-' {
                    state = HtmlScriptDataState::EscapedDashDash;
                } else {
                    state = HtmlScriptDataState::Data;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::Escaped => match byte {
                b'-' => state = HtmlScriptDataState::EscapedDash,
                b'<' => state = HtmlScriptDataState::EscapedLessThan,
                _ => {}
            },
            HtmlScriptDataState::EscapedDash => match byte {
                b'-' => state = HtmlScriptDataState::EscapedDashDash,
                b'<' => state = HtmlScriptDataState::EscapedLessThan,
                _ => state = HtmlScriptDataState::Escaped,
            },
            HtmlScriptDataState::EscapedDashDash => match byte {
                b'-' => {}
                b'<' => state = HtmlScriptDataState::EscapedLessThan,
                b'>' => state = HtmlScriptDataState::Data,
                _ => state = HtmlScriptDataState::Escaped,
            },
            HtmlScriptDataState::EscapedLessThan => match byte {
                b'/' => {
                    tag_start = position.saturating_sub(1);
                    temporary_buffer.clear();
                    state = HtmlScriptDataState::EscapedEndTagOpen;
                }
                value if value.is_ascii_alphabetic() => {
                    temporary_buffer.clear();
                    state = HtmlScriptDataState::DoubleEscapeStart;
                    reconsume = true;
                }
                _ => {
                    state = HtmlScriptDataState::Escaped;
                    reconsume = true;
                }
            },
            HtmlScriptDataState::EscapedEndTagOpen => {
                if byte.is_ascii_alphabetic() {
                    state = HtmlScriptDataState::EscapedEndTagName;
                    reconsume = true;
                } else {
                    state = HtmlScriptDataState::Escaped;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::EscapedEndTagName => {
                if byte.is_ascii_alphabetic() {
                    temporary_buffer.push(byte.to_ascii_lowercase());
                } else if is_delimiter(byte) {
                    if temporary_buffer == b"script" {
                        return find_tag_end(source, position).map(|_| tag_start);
                    }
                    state = HtmlScriptDataState::Escaped;
                    reconsume = true;
                } else {
                    state = HtmlScriptDataState::Escaped;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::DoubleEscapeStart => {
                if byte.is_ascii_alphabetic() {
                    temporary_buffer.push(byte.to_ascii_lowercase());
                } else if is_delimiter(byte) {
                    state = if temporary_buffer == b"script" {
                        HtmlScriptDataState::DoubleEscaped
                    } else {
                        HtmlScriptDataState::Escaped
                    };
                } else {
                    state = HtmlScriptDataState::Escaped;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::DoubleEscaped => match byte {
                b'-' => state = HtmlScriptDataState::DoubleEscapedDash,
                b'<' => state = HtmlScriptDataState::DoubleEscapedLessThan,
                _ => {}
            },
            HtmlScriptDataState::DoubleEscapedDash => match byte {
                b'-' => state = HtmlScriptDataState::DoubleEscapedDashDash,
                b'<' => state = HtmlScriptDataState::DoubleEscapedLessThan,
                _ => state = HtmlScriptDataState::DoubleEscaped,
            },
            HtmlScriptDataState::DoubleEscapedDashDash => match byte {
                b'-' => {}
                b'<' => state = HtmlScriptDataState::DoubleEscapedLessThan,
                b'>' => state = HtmlScriptDataState::Data,
                _ => state = HtmlScriptDataState::DoubleEscaped,
            },
            HtmlScriptDataState::DoubleEscapedLessThan => {
                if byte == b'/' {
                    temporary_buffer.clear();
                    state = HtmlScriptDataState::DoubleEscapeEnd;
                } else {
                    state = HtmlScriptDataState::DoubleEscaped;
                    reconsume = true;
                }
            }
            HtmlScriptDataState::DoubleEscapeEnd => {
                if byte.is_ascii_alphabetic() {
                    temporary_buffer.push(byte.to_ascii_lowercase());
                } else if is_delimiter(byte) {
                    state = if temporary_buffer == b"script" {
                        HtmlScriptDataState::Escaped
                    } else {
                        HtmlScriptDataState::DoubleEscaped
                    };
                } else {
                    state = HtmlScriptDataState::DoubleEscaped;
                    reconsume = true;
                }
            }
        }

        if !reconsume {
            position += 1;
        }
    }

    None
}

pub(super) fn is_void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}
