use super::*;

pub(super) fn parse_background_color_value(value: &str) -> Option<NativeBackgroundColorValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("currentColor") {
        Some(NativeBackgroundColorValue::CurrentColor)
    } else if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBackgroundColorValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBackgroundColorValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBackgroundColorValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBackgroundColorValue::Revert)
    } else if let Some(value) = parse_color_custom_property(value) {
        match value {
            NativeColorValue::Color(color) => Some(NativeBackgroundColorValue::Color(color)),
            NativeColorValue::CurrentColor => Some(NativeBackgroundColorValue::CurrentColor),
            NativeColorValue::Inherit => Some(NativeBackgroundColorValue::Inherit),
            NativeColorValue::Unset => Some(NativeBackgroundColorValue::Unset),
            NativeColorValue::Initial => Some(NativeBackgroundColorValue::Initial),
            NativeColorValue::Revert => Some(NativeBackgroundColorValue::Revert),
            NativeColorValue::CustomProperty(name_hash) => {
                Some(NativeBackgroundColorValue::CustomProperty(name_hash))
            }
            NativeColorValue::CustomPropertyFallback(name_hash, fallback) => Some(
                NativeBackgroundColorValue::CustomPropertyFallback(name_hash, fallback),
            ),
        }
    } else {
        parse_color(value).map(NativeBackgroundColorValue::Color)
    }
}

pub(super) fn parse_background_color_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBackgroundColorValue>> {
    parse_local_cascade_declaration(value, parse_background_color_value)
}

pub(super) fn background_image_source_id(source: &str) -> u32 {
    let mut hash = 2_166_136_261u32;
    for byte in source.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    if hash == 0 { 1 } else { hash }
}

pub(super) fn parse_background_image_url(value: &str) -> Option<&str> {
    let value = value.trim();
    let open = value.find('(')?;
    if !value[..open].trim().eq_ignore_ascii_case("url") || !value.ends_with(')') {
        return None;
    }
    let inner = value[open + 1..value.len().saturating_sub(1)].trim();
    if inner.len() > 2 {
        let bytes = inner.as_bytes();
        if matches!(bytes.first(), Some(b'"' | b'\'')) {
            if bytes.last() != bytes.first() {
                return None;
            }
            let inner = &inner[1..inner.len().saturating_sub(1)];
            if inner.is_empty()
                || inner.len() > 1024
                || inner.bytes().any(|byte| byte.is_ascii_control())
            {
                return None;
            }
            return Some(inner);
        }
    }
    if inner.is_empty()
        || inner.len() > 1024
        || inner
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return None;
    }
    Some(inner)
}

pub(super) fn parse_background_image_concrete_value(
    value: &str,
) -> Option<NativeBackgroundImageValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(NativeBackgroundImageValue::None);
    }
    parse_background_image_url(value)
        .map(background_image_source_id)
        .map(NativeBackgroundImageValue::Url)
}

pub(super) fn parse_background_image_value(value: &str) -> Option<NativeBackgroundImageValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("inherit") {
        return Some(NativeBackgroundImageValue::Inherit);
    }
    if value.eq_ignore_ascii_case("unset") {
        return Some(NativeBackgroundImageValue::Unset);
    }
    if value.eq_ignore_ascii_case("initial") {
        return Some(NativeBackgroundImageValue::Initial);
    }
    if value.eq_ignore_ascii_case("revert") {
        return Some(NativeBackgroundImageValue::Revert);
    }
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => match parse_background_image_concrete_value(fallback)? {
                NativeBackgroundImageValue::Url(source) => Some(
                    NativeBackgroundImageValue::CustomPropertyFallback(name_hash, Some(source)),
                ),
                NativeBackgroundImageValue::None => Some(
                    NativeBackgroundImageValue::CustomPropertyFallback(name_hash, None),
                ),
                NativeBackgroundImageValue::Inherit
                | NativeBackgroundImageValue::Unset
                | NativeBackgroundImageValue::Initial
                | NativeBackgroundImageValue::Revert
                | NativeBackgroundImageValue::CustomProperty(_)
                | NativeBackgroundImageValue::CustomPropertyFallback(_, _)
                | NativeBackgroundImageValue::CustomPropertyShorthand(_)
                | NativeBackgroundImageValue::CustomPropertyShorthandFallback(_, _) => None,
            },
            None => Some(NativeBackgroundImageValue::CustomProperty(name_hash)),
        };
    }
    parse_background_image_concrete_value(value)
}

pub(super) fn parse_background_image_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBackgroundImageValue>> {
    parse_local_cascade_declaration(value, parse_background_image_value)
}

pub(super) fn parse_background_repeat(value: &str) -> Option<NativeBackgroundRepeat> {
    let tokens = value
        .split_ascii_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    match tokens.as_slice() {
        [repeat] => match repeat.as_str() {
            "repeat" => Some(NativeBackgroundRepeat::Repeat),
            "repeat-x" => Some(NativeBackgroundRepeat::RepeatX),
            "repeat-y" => Some(NativeBackgroundRepeat::RepeatY),
            "no-repeat" => Some(NativeBackgroundRepeat::NoRepeat),
            _ => None,
        },
        [horizontal, vertical] => match (horizontal.as_str(), vertical.as_str()) {
            ("repeat", "repeat") => Some(NativeBackgroundRepeat::Repeat),
            ("repeat", "no-repeat") => Some(NativeBackgroundRepeat::RepeatX),
            ("no-repeat", "repeat") => Some(NativeBackgroundRepeat::RepeatY),
            ("no-repeat", "no-repeat") => Some(NativeBackgroundRepeat::NoRepeat),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn parse_background_repeat_fallback(
    value: &str,
) -> Option<NativeBackgroundRepeatFallback> {
    match parse_background_repeat(value)? {
        NativeBackgroundRepeat::Repeat => Some(NativeBackgroundRepeatFallback::Repeat),
        NativeBackgroundRepeat::RepeatX => Some(NativeBackgroundRepeatFallback::RepeatX),
        NativeBackgroundRepeat::RepeatY => Some(NativeBackgroundRepeatFallback::RepeatY),
        NativeBackgroundRepeat::NoRepeat => Some(NativeBackgroundRepeatFallback::NoRepeat),
        NativeBackgroundRepeat::CustomProperty(_)
        | NativeBackgroundRepeat::CustomPropertyFallback(_, _)
        | NativeBackgroundRepeat::CustomPropertyShorthand(_)
        | NativeBackgroundRepeat::CustomPropertyShorthandFallback(_, _) => None,
    }
}

pub(super) fn parse_background_repeat_value(value: &str) -> Option<NativeBackgroundRepeat> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_background_repeat_fallback(fallback).map(|fallback| {
                NativeBackgroundRepeat::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeBackgroundRepeat::CustomProperty(name_hash)),
        };
    }
    parse_background_repeat(value)
}

pub(super) fn parse_background_repeat_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBackgroundRepeat>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_background_repeat_value)
}

pub(super) fn parse_background_percentage(value: &str) -> Option<i32> {
    let value = value.trim();
    let value = value.strip_suffix('%')?.trim();
    let (negative, value) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let milli = i64::from(parse_decimal_milli(value)?);
    let scaled = (milli + 50) / 100;
    let scaled = if negative { -scaled } else { scaled };
    (scaled.abs() <= i64::from(MAX_NATIVE_BACKGROUND_PERCENT))
        .then(|| i32::try_from(scaled).ok())
        .flatten()
}

pub(super) fn parse_background_pixel(value: &str) -> Option<i32> {
    let value = value.trim().to_ascii_lowercase();
    let (negative, value) = value
        .strip_prefix('-')
        .map_or((false, value.as_str()), |value| (true, value));
    let value = if value == "0" {
        value
    } else {
        value.strip_suffix("px")?.trim()
    };
    let milli = i64::from(parse_decimal_milli(value)?);
    let pixels = (milli + 500) / 1_000;
    let pixels = if negative { -pixels } else { pixels };
    (pixels.abs() <= i64::from(MAX_NATIVE_VIEWPORT_DIMENSION))
        .then(|| i32::try_from(pixels).ok())
        .flatten()
}

pub(super) fn parse_background_position_axis_component(
    value: &str,
    horizontal: bool,
) -> Option<NativeBackgroundPositionComponent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" if horizontal => Some(NativeBackgroundPositionComponent::Percentage(0)),
        "right" if horizontal => Some(NativeBackgroundPositionComponent::Percentage(
            NATIVE_BACKGROUND_PERCENT_SCALE,
        )),
        "top" if !horizontal => Some(NativeBackgroundPositionComponent::Percentage(0)),
        "bottom" if !horizontal => Some(NativeBackgroundPositionComponent::Percentage(
            NATIVE_BACKGROUND_PERCENT_SCALE,
        )),
        "center" => Some(NativeBackgroundPositionComponent::Percentage(
            NATIVE_BACKGROUND_PERCENT_SCALE / 2,
        )),
        value if value.ends_with('%') => {
            parse_background_percentage(value).map(NativeBackgroundPositionComponent::Percentage)
        }
        value => parse_background_pixel(value).map(NativeBackgroundPositionComponent::Length),
    }
}

pub(super) fn parse_background_position(value: &str) -> Option<NativeBackgroundPosition> {
    let tokens = split_css_value_tokens(value)?;
    match tokens.as_slice() {
        [value] => {
            if let Some(x) = parse_background_position_axis_component(value, true) {
                Some(NativeBackgroundPosition {
                    x,
                    y: NativeBackgroundPositionComponent::Percentage(
                        NATIVE_BACKGROUND_PERCENT_SCALE / 2,
                    ),
                })
            } else {
                parse_background_position_axis_component(value, false).map(|y| {
                    NativeBackgroundPosition {
                        x: NativeBackgroundPositionComponent::Percentage(
                            NATIVE_BACKGROUND_PERCENT_SCALE / 2,
                        ),
                        y,
                    }
                })
            }
        }
        [first, second] => {
            if let (Some(x), Some(y)) = (
                parse_background_position_axis_component(first, true),
                parse_background_position_axis_component(second, false),
            ) {
                Some(NativeBackgroundPosition { x, y })
            } else if let (Some(y), Some(x)) = (
                parse_background_position_axis_component(first, false),
                parse_background_position_axis_component(second, true),
            ) {
                Some(NativeBackgroundPosition { x, y })
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn parse_background_position_value(
    value: &str,
) -> Option<NativeBackgroundPositionDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_background_position(fallback).map(|fallback| {
                NativeBackgroundPositionDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeBackgroundPositionDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    parse_background_position(value).map(NativeBackgroundPositionDeclaration::Value)
}

pub(super) fn parse_background_position_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBackgroundPositionDeclaration>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_background_position_value)
}

pub(super) fn parse_background_size_component(
    value: &str,
) -> Option<NativeBackgroundSizeComponent> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Some(NativeBackgroundSizeComponent::Auto);
    }
    if value.ends_with('%') {
        return u32::try_from(parse_background_percentage(value).filter(|value| *value >= 0)?)
            .ok()
            .map(NativeBackgroundSizeComponent::Percentage);
    }
    u32::try_from(parse_background_pixel(value).filter(|value| *value >= 0)?)
        .ok()
        .map(NativeBackgroundSizeComponent::Length)
}

pub(super) fn parse_background_size(value: &str) -> Option<NativeBackgroundSize> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("cover") {
        return Some(NativeBackgroundSize::Cover);
    }
    if value.eq_ignore_ascii_case("contain") {
        return Some(NativeBackgroundSize::Contain);
    }
    let tokens = split_css_value_tokens(value)?;
    match tokens.as_slice() {
        [width] => Some(NativeBackgroundSize::Explicit {
            width: parse_background_size_component(width)?,
            height: NativeBackgroundSizeComponent::Auto,
        }),
        [width, height] => Some(NativeBackgroundSize::Explicit {
            width: parse_background_size_component(width)?,
            height: parse_background_size_component(height)?,
        }),
        _ => None,
    }
}

pub(super) fn parse_background_size_value(value: &str) -> Option<NativeBackgroundSizeDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_background_size(fallback).map(|fallback| {
                NativeBackgroundSizeDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeBackgroundSizeDeclaration::CustomProperty(name_hash)),
        };
    }
    parse_background_size(value).map(NativeBackgroundSizeDeclaration::Value)
}

pub(super) fn parse_background_size_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBackgroundSizeDeclaration>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_background_size_value)
}

pub(super) fn split_background_size_separator(value: &str) -> Option<(&str, Option<&str>)> {
    let mut parentheses = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut separator = None;
    for (index, byte) in value.bytes().enumerate() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => parentheses = parentheses.checked_add(1)?,
            b')' => parentheses = parentheses.checked_sub(1)?,
            b'/' if parentheses == 0 && separator.replace(index).is_some() => {
                return None;
            }
            _ => {}
        }
    }
    if parentheses != 0 || quote.is_some() {
        return None;
    }
    match separator {
        Some(index) => {
            let before = value[..index].trim();
            let after = value[index.saturating_add(1)..].trim();
            (!before.is_empty() && !after.is_empty()).then_some((before, Some(after)))
        }
        None => Some((value.trim(), None)),
    }
}

pub(super) fn parse_background_shorthand(value: &str) -> Option<NativeBackgroundShorthand> {
    let (before_size, after_size) = split_background_size_separator(value)?;
    let tokens = split_css_value_tokens(before_size)?;
    let mut color = None;
    let mut image = None;
    let mut repeat = None;
    let mut position_tokens = Vec::new();
    for token in tokens {
        if image.is_none()
            && let Some(parsed) = parse_background_image_value(token)
            && matches!(
                parsed,
                NativeBackgroundImageValue::Url(_) | NativeBackgroundImageValue::None
            )
        {
            image = Some(parsed);
            continue;
        }
        if repeat.is_none()
            && let Some(parsed) = parse_background_repeat(token)
        {
            repeat = Some(parsed);
            continue;
        }
        if color.is_none()
            && let Some(parsed) = parse_background_color_value(token)
            && matches!(
                parsed,
                NativeBackgroundColorValue::Color(_) | NativeBackgroundColorValue::CurrentColor
            )
        {
            color = Some(parsed);
            continue;
        }
        position_tokens.push(token);
    }

    let position = if position_tokens.is_empty() {
        NativeBackgroundPosition::default()
    } else {
        parse_background_position(&position_tokens.join(" "))?
    };
    let mut size_tokens = match after_size {
        Some(value) => split_css_value_tokens(value)?,
        None => Vec::new(),
    };
    if repeat.is_none()
        && let Some(last) = size_tokens.last().copied()
        && let Some(parsed) = parse_background_repeat(last)
    {
        repeat = Some(parsed);
        size_tokens.pop();
    }
    let size = if size_tokens.is_empty() {
        NativeBackgroundSize::default()
    } else {
        parse_background_size(&size_tokens.join(" "))?
    };
    Some(NativeBackgroundShorthand {
        color: color.unwrap_or(NativeBackgroundColorValue::Color(NativeColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        })),
        image: image.unwrap_or(NativeBackgroundImageValue::None),
        repeat: repeat.unwrap_or_default(),
        position,
        size,
    })
}

pub(super) fn parse_background_shorthand_declaration(
    value: &str,
) -> Option<NativeBackgroundShorthandDeclaration> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeBackgroundShorthandDeclaration::RevertLayer);
    }
    if value.eq_ignore_ascii_case("inherit") {
        return Some(NativeBackgroundShorthandDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(NativeBackgroundShorthandDeclaration::Reset);
    }
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_background_shorthand(fallback).map(|fallback| {
                NativeBackgroundShorthandDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeBackgroundShorthandDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    parse_background_shorthand(value).map(NativeBackgroundShorthandDeclaration::Value)
}

pub(crate) fn collect_background_image_sources(
    source: &str,
    background_image_sources: &mut BTreeMap<u32, String>,
) {
    for (_, declaration) in split_css_declarations(source) {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let (value, _) = strip_important_suffix(value);
        if property.trim().eq_ignore_ascii_case("background-image") {
            if let Some(source) = parse_background_image_url(value) {
                register_background_image_source(source, background_image_sources);
            }
        } else if property.trim().eq_ignore_ascii_case("background")
            && parse_background_shorthand(value).is_some()
            && let Some(tokens) = split_css_value_tokens(value)
        {
            for token in tokens {
                if let Some(source) = parse_background_image_url(token) {
                    register_background_image_source(source, background_image_sources);
                }
            }
        }
    }
}

/// Resolve relative CSS `url(...)` tokens against the loaded stylesheet URL.
/// The CSS parser stores background-image identities as source strings, so
/// canonicalizing before parsing gives each stylesheet its own URL base
/// without changing the declaration/cascade model. File and HTTP(S) owners
/// are kept on their corresponding resource paths; blob, data, fixture, and
/// unsupported owners are returned byte-for-byte unchanged.
pub(in crate::native_engine) fn absolutize_stylesheet_urls(
    source: &str,
    document_url: &str,
    stylesheet_href: &str,
) -> String {
    let Some(base_url) = stylesheet_base_url(document_url, stylesheet_href) else {
        return source.to_owned();
    };
    let bytes = source.as_bytes();
    let mut rewritten = String::with_capacity(source.len());
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            let end = source[cursor + 2..]
                .find("*/")
                .map(|offset| cursor + 2 + offset + 2)
                .unwrap_or(bytes.len());
            rewritten.push_str(&source[cursor..end]);
            cursor = end;
            continue;
        }
        if matches!(bytes[cursor], b'\'' | b'"') {
            let quote = bytes[cursor];
            let mut end = cursor + 1;
            let mut escaped = false;
            while end < bytes.len() {
                let byte = bytes[end];
                end += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == quote {
                    break;
                }
            }
            rewritten.push_str(&source[cursor..end]);
            cursor = end;
            continue;
        }
        let Some(open) = css_url_function_open(bytes, cursor) else {
            let character = source[cursor..]
                .chars()
                .next()
                .expect("CSS cursor must point at a character");
            let end = cursor + character.len_utf8();
            rewritten.push_str(&source[cursor..end]);
            cursor = end;
            continue;
        };
        let Some(close) = css_url_function_close(bytes, open + 1) else {
            let character = source[cursor..]
                .chars()
                .next()
                .expect("CSS cursor must point at a character");
            let end = cursor + character.len_utf8();
            rewritten.push_str(&source[cursor..end]);
            cursor = end;
            continue;
        };
        let mut value_start = open + 1;
        while value_start < close && bytes[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        let mut value_end = close;
        while value_end > value_start && bytes[value_end - 1].is_ascii_whitespace() {
            value_end -= 1;
        }
        rewritten.push_str(&source[cursor..value_start]);
        if value_start < value_end {
            let token = &source[value_start..value_end];
            if let Some(resolved) = resolve_css_url_token(token, &base_url) {
                rewritten.push_str(&resolved);
            } else {
                rewritten.push_str(token);
            }
        }
        rewritten.push_str(&source[value_end..=close]);
        cursor = close + 1;
    }
    rewritten
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeCssImport {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) specifier: String,
    pub(crate) layer: Option<String>,
    pub(crate) supports: Option<String>,
    pub(crate) media: Option<String>,
}

/// Return literal CSS `@import` statements with their bounded conditional
/// preludes. The caller owns URL resolution and graph limits; this pass only
/// discovers quoted or `url(...)` targets outside comments/strings. A
/// malformed import is rejected instead of being silently left for the
/// ordinary CSS parser, so a rooted stylesheet cannot partially apply around
/// a failed dependency.
pub(in crate::native_engine) fn static_css_imports(
    source: &str,
) -> Result<Vec<NativeCssImport>, &'static str> {
    let bytes = source.as_bytes();
    let mut imports = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            cursor = css_comment_end(bytes, cursor);
            continue;
        }
        if matches!(bytes[cursor], b'\'' | b'"') {
            cursor = css_quoted_end(bytes, cursor);
            continue;
        }
        const IMPORT: &[u8] = b"@import";
        let keyword_end = cursor.saturating_add(IMPORT.len());
        if keyword_end <= bytes.len()
            && bytes[cursor..keyword_end].eq_ignore_ascii_case(IMPORT)
            && (cursor == 0 || !is_css_identifier_byte(bytes[cursor - 1]))
            && (keyword_end == bytes.len() || !is_css_identifier_byte(bytes[keyword_end]))
            && let Some(statement_end) = css_statement_end(bytes, keyword_end)
        {
            let (specifier, layer, supports, media) =
                css_import_parts(&source[keyword_end..statement_end], cursor)
                    .ok_or("CSS @import statement has no supported literal URL")?;
            imports.push(NativeCssImport {
                start: cursor,
                end: statement_end + 1,
                specifier,
                layer,
                supports,
                media,
            });
            cursor = statement_end + 1;
            continue;
        }
        if keyword_end <= bytes.len()
            && bytes[cursor..keyword_end].eq_ignore_ascii_case(IMPORT)
            && (cursor == 0 || !is_css_identifier_byte(bytes[cursor - 1]))
            && (keyword_end == bytes.len() || !is_css_identifier_byte(bytes[keyword_end]))
        {
            return Err("CSS @import statement is missing a terminating semicolon");
        }
        cursor += 1;
    }
    Ok(imports)
}
