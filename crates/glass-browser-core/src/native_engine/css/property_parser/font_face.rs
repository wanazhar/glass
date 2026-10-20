use super::*;

pub(crate) fn parse_font_weight_range(value: &str) -> Option<NativeFontWeightRange> {
    let mut values = value.split_ascii_whitespace();
    let min = parse_font_weight(values.next()?)?.numeric();
    let max = match values.next() {
        Some(value) => parse_font_weight(value)?.numeric(),
        None => min,
    };
    if values.next().is_some() || min > max {
        return None;
    }
    Some(NativeFontWeightRange { min, max })
}

pub(in crate::native_engine::css) fn parse_font_style(value: &str) -> Option<FontStyleValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(FontStyleValue::Normal),
        "italic" => Some(FontStyleValue::Italic),
        _ => None,
    }
}
pub(in crate::native_engine::css) fn parse_font_display(value: &str) -> Option<NativeFontDisplay> {
    NativeFontDisplay::parse(value)
}
pub(crate) fn parse_font_size_adjust(value: &str) -> Option<u16> {
    let percentage = value.trim().strip_suffix('%')?.trim();
    let milli_percentage = parse_decimal_milli(percentage)?;
    let adjusted = milli_percentage.saturating_add(50) / 100;
    let adjusted = u16::try_from(adjusted).ok()?;
    (250..=4000).contains(&adjusted).then_some(adjusted)
}

pub(crate) fn format_font_size_adjust(value: u16) -> String {
    if value.is_multiple_of(10) {
        format!("{}%", value / 10)
    } else {
        format!("{}.{}%", value / 10, value % 10)
    }
}

pub(crate) fn parse_font_metric_override(value: &str) -> Option<Option<u16>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("normal") {
        return Some(None);
    }
    let percentage = value.strip_suffix('%')?;
    let milli_percentage = parse_decimal_milli(percentage)?;
    let scaled = milli_percentage.saturating_add(50) / 100;
    let scaled = u16::try_from(scaled).ok()?;
    (scaled <= 10_000).then_some(Some(scaled))
}

pub(crate) fn format_font_metric_override(value: Option<u16>) -> String {
    value.map_or_else(|| "normal".into(), format_font_size_adjust)
}

pub(in crate::native_engine::css) fn parse_font_stretch_token(value: &str) -> Option<u16> {
    let value = value.trim();
    let keyword = match value.to_ascii_lowercase().as_str() {
        "ultra-condensed" => Some(500),
        "extra-condensed" => Some(625),
        "condensed" => Some(750),
        "semi-condensed" => Some(875),
        "normal" => Some(1000),
        "semi-expanded" => Some(1125),
        "expanded" => Some(1250),
        "extra-expanded" => Some(1500),
        "ultra-expanded" => Some(2000),
        _ => None,
    };
    if let Some(keyword) = keyword {
        return Some(keyword);
    }
    let number = value.strip_suffix('%')?;
    let scaled_milli = parse_decimal_milli(number)?;
    let scaled_tenths = scaled_milli.saturating_add(50) / 100;
    let scaled_tenths = u16::try_from(scaled_tenths).ok()?;
    (500..=2000)
        .contains(&scaled_tenths)
        .then_some(scaled_tenths)
}

pub(crate) fn parse_font_stretch_range(value: &str) -> Option<NativeFontStretchRange> {
    let mut values = value.split_ascii_whitespace();
    let min = parse_font_stretch_token(values.next()?)?;
    let max = match values.next() {
        Some(value) => parse_font_stretch_token(value)?,
        None => min,
    };
    if values.next().is_some() || min > max {
        return None;
    }
    Some(NativeFontStretchRange { min, max })
}

/// Parse the single-value `font-stretch` property into percentage tenths.
/// The two-value form is reserved for `@font-face` descriptor ranges.
pub(crate) fn parse_font_stretch_value(value: &str) -> Option<u16> {
    let mut values = value.split_ascii_whitespace();
    let stretch = parse_font_stretch_token(values.next()?)?;
    values.next().is_none().then_some(stretch)
}

pub(crate) fn format_font_stretch_range(range: NativeFontStretchRange) -> String {
    if range.min == range.max {
        let keyword = match range.min {
            500 => Some("ultra-condensed"),
            625 => Some("extra-condensed"),
            750 => Some("condensed"),
            875 => Some("semi-condensed"),
            1000 => Some("normal"),
            1125 => Some("semi-expanded"),
            1250 => Some("expanded"),
            1500 => Some("extra-expanded"),
            2000 => Some("ultra-expanded"),
            _ => None,
        };
        if let Some(keyword) = keyword {
            return keyword.into();
        }
    }
    let format_percentage = |value: u16| {
        if value.is_multiple_of(10) {
            format!("{}%", value / 10)
        } else {
            format!("{}.{}%", value / 10, value % 10)
        }
    };
    if range.min == range.max {
        format_percentage(range.min)
    } else {
        format!(
            "{} {}",
            format_percentage(range.min),
            format_percentage(range.max)
        )
    }
}

pub(crate) fn parse_font_face_unicode_range(value: &str) -> Option<Vec<NativeUnicodeRange>> {
    let mut ranges = Vec::new();
    for token in value.split(',') {
        let token = token.trim();
        if token.is_empty() || token.len() > 16 {
            return None;
        }
        let prefix = token.get(..2)?;
        if !prefix.eq_ignore_ascii_case("u+") {
            return None;
        }
        let value = &token[2..];
        let (start, end) = if let Some((start, end)) = value.split_once('-') {
            if start.is_empty() || end.is_empty() {
                return None;
            }
            (
                u32::from_str_radix(start, 16).ok()?,
                u32::from_str_radix(end, 16).ok()?,
            )
        } else if value.contains('?') {
            let mut saw_question = false;
            let mut start_text = String::with_capacity(value.len());
            let mut end_text = String::with_capacity(value.len());
            for character in value.chars() {
                if character == '?' {
                    saw_question = true;
                    start_text.push('0');
                    end_text.push('F');
                } else {
                    if saw_question || !character.is_ascii_hexdigit() {
                        return None;
                    }
                    start_text.push(character);
                    end_text.push(character);
                }
            }
            if start_text.is_empty() {
                return None;
            }
            (
                u32::from_str_radix(&start_text, 16).ok()?,
                u32::from_str_radix(&end_text, 16).ok()?,
            )
        } else {
            let point = u32::from_str_radix(value, 16).ok()?;
            (point, point)
        };
        if start > end || end > 0x10_FFFF {
            return None;
        }
        ranges.push(NativeUnicodeRange { start, end });
        if ranges.len() > MAX_NATIVE_FONT_FACE_UNICODE_RANGES {
            return None;
        }
    }
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<NativeUnicodeRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end.saturating_add(1)
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    Some(merged)
}

pub(crate) fn format_font_face_unicode_ranges(ranges: &[NativeUnicodeRange]) -> String {
    if ranges.is_empty() {
        return "U+0-10FFFF".into();
    }
    ranges
        .iter()
        .map(|range| {
            if range.start == range.end {
                format!("U+{:X}", range.start)
            } else {
                format!("U+{:X}-{:X}", range.start, range.end)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in crate::native_engine::css) fn parse_font_family(
    value: &str,
) -> Option<NativeFontFamilyList> {
    NativeFontFamilyList::parse(value)
}

pub(in crate::native_engine::css) fn parse_font_face_family(value: &str) -> Option<String> {
    let value = value.trim();
    let family = if value.len() >= 2
        && matches!(value.as_bytes().first(), Some(b'"' | b'\''))
        && value.as_bytes().last() == value.as_bytes().first()
    {
        value[1..value.len().saturating_sub(1)].trim()
    } else {
        value
    };
    let families = parse_font_family(value)?;
    let mut parsed = families.iter();
    if !matches!(parsed.next(), Some(NativeFontFamilyValue::Named(_)))
        || parsed.next().is_some()
        || family.is_empty()
        || family.bytes().any(|byte| byte.is_ascii_control())
    {
        return None;
    }
    Some(family.to_owned())
}

pub(in crate::native_engine::css) fn parse_font_face_src(
    value: &str,
) -> Option<Vec<NativeFontFaceSource>> {
    let bytes = value.as_bytes();
    let mut cursor = 0usize;
    let mut sources = Vec::new();
    while cursor < bytes.len() {
        let Some((open, is_url)) = css_function_candidate(bytes, cursor) else {
            let character = value[cursor..]
                .chars()
                .next()
                .expect("CSS source cursor must point at a character");
            cursor = cursor.saturating_add(character.len_utf8());
            continue;
        };
        let close = css_url_function_close(bytes, open.saturating_add(1))?;
        let mut start = open.saturating_add(1);
        while start < close && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        let mut end = close;
        while end > start && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let token = value.get(start..end)?;
        if is_url {
            let token = if token.len() >= 2
                && matches!(token.as_bytes().first(), Some(b'"' | b'\''))
                && token.as_bytes().last() == token.as_bytes().first()
            {
                &token[1..token.len().saturating_sub(1)]
            } else {
                token
            };
            let token = decode_css_url_value(token)?;
            if is_admissible_font_face_source(&token)
                && !token.bytes().any(|byte| byte.is_ascii_control())
                && token.len() <= MAX_NATIVE_FONT_FACE_SOURCE_BYTES
                && sources.len() < MAX_NATIVE_FONT_FACE_SOURCES
            {
                sources.push(NativeFontFaceSource::Url(token));
            }
        } else if let Some(family) = parse_font_face_local(token)
            && sources.len() < MAX_NATIVE_FONT_FACE_SOURCES
        {
            sources.push(NativeFontFaceSource::Local(family));
        }
        cursor = close.saturating_add(1);
    }
    (!sources.is_empty()).then_some(sources)
}

pub(in crate::native_engine::css) fn parse_font_face_local(value: &str) -> Option<String> {
    parse_font_face_family(value.trim())
}

pub(in crate::native_engine::css) fn css_function_candidate(
    bytes: &[u8],
    cursor: usize,
) -> Option<(usize, bool)> {
    if let Some(open) = css_named_function_open(bytes, cursor, b"url") {
        return Some((open, true));
    }
    css_named_function_open(bytes, cursor, b"local").map(|open| (open, false))
}

pub(in crate::native_engine::css) fn is_admissible_font_face_source(value: &str) -> bool {
    if value.is_empty() || value.starts_with('#') || value.bytes().any(|byte| byte == 0) {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        // Relative URLs and protocol-relative URLs are resolved by the
        // document's resource owner after stylesheet URL canonicalization.
        return !value.contains(':');
    };
    matches!(
        url.scheme().to_ascii_lowercase().as_str(),
        "blob" | "data" | "file" | "http" | "https"
    )
}

pub(in crate::native_engine::css) fn parse_font_family_name(
    value: &str,
) -> Option<NativeFontFamilyValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sans-serif" | "ui-sans-serif" => Some(NativeFontFamilyValue::Generic(
            NativeGenericFontFamily::SansSerif,
        )),
        "serif" | "ui-serif" => Some(NativeFontFamilyValue::Generic(
            NativeGenericFontFamily::Serif,
        )),
        "monospace" | "ui-monospace" => Some(NativeFontFamilyValue::Generic(
            NativeGenericFontFamily::Monospace,
        )),
        "cursive" => Some(NativeFontFamilyValue::Generic(
            NativeGenericFontFamily::Cursive,
        )),
        "fantasy" => Some(NativeFontFamilyValue::Generic(
            NativeGenericFontFamily::Fantasy,
        )),
        "system-ui" => Some(NativeFontFamilyValue::Generic(
            NativeGenericFontFamily::SystemUi,
        )),
        _ => (!value.trim().is_empty())
            .then_some(NativeFontFamilyValue::Named(font_family_hash(value))),
    }
}

pub(crate) fn font_family_hash(value: &str) -> u64 {
    let mut hash = 1469598103934665603u64;
    for byte in value
        .trim()
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
    {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}
