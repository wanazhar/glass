use super::*;

pub(in crate::native_engine::css) fn parse_text_decoration(
    value: &str,
) -> Option<TextDecorationValue> {
    let mut bits = 0;
    let mut token_count = 0;
    let mut saw_none = false;

    for token in value.split_ascii_whitespace() {
        token_count += 1;
        if token.eq_ignore_ascii_case("none") {
            if token_count != 1 {
                return None;
            }
            saw_none = true;
            continue;
        }
        if saw_none {
            return None;
        }
        let bit = if token.eq_ignore_ascii_case("underline") {
            TextDecorationValue::UNDERLINE
        } else if token.eq_ignore_ascii_case("overline") {
            TextDecorationValue::OVERLINE
        } else if token.eq_ignore_ascii_case("line-through") {
            TextDecorationValue::LINE_THROUGH
        } else {
            return None;
        };
        if bits & bit != 0 {
            return None;
        }
        bits |= bit;
    }

    if token_count == 0 {
        None
    } else if saw_none {
        Some(TextDecorationValue::none())
    } else {
        Some(TextDecorationValue::new(
            bits & TextDecorationValue::UNDERLINE != 0,
            bits & TextDecorationValue::OVERLINE != 0,
            bits & TextDecorationValue::LINE_THROUGH != 0,
        ))
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_declaration(
    value: &str,
) -> Option<NativeTextDecorationDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_text_decoration(fallback).map(|fallback| {
                NativeTextDecorationDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeTextDecorationDeclaration::CustomProperty(name_hash)),
        };
    }
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Some(NativeTextDecorationDeclaration::Inherit),
        "initial" => Some(NativeTextDecorationDeclaration::Initial),
        "unset" => Some(NativeTextDecorationDeclaration::Unset),
        "revert" => Some(NativeTextDecorationDeclaration::Revert),
        "revert-layer" => Some(NativeTextDecorationDeclaration::RevertLayer),
        _ => parse_text_decoration(value).map(NativeTextDecorationDeclaration::Value),
    }
}

pub(in crate::native_engine::css) fn parse_text_transform(
    value: &str,
) -> Option<TextTransformValue> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(TextTransformValue::None),
        "uppercase" => Some(TextTransformValue::Uppercase),
        "lowercase" => Some(TextTransformValue::Lowercase),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_weight(value: &str) -> Option<FontWeightValue> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "normal" | "400" => Some(FontWeightValue::Normal),
        "bold" | "700" => Some(FontWeightValue::Bold),
        _ => value
            .parse::<u16>()
            .ok()
            .and_then(FontWeightValue::from_numeric),
    }
}
