use super::*;

pub(in crate::native_engine::css) fn parse_line_height(value: &str) -> Option<u32> {
    parse_dimension(value).filter(|value| *value > 0)
}

pub(in crate::native_engine::css) fn parse_line_height_custom_property(
    value: &str,
) -> Option<LineHeightDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_line_height(fallback).map(|fallback| {
            LineHeightDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(LineHeightDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_line_height_property(
    value: &str,
) -> Option<LineHeightDeclarationValue> {
    parse_line_height(value)
        .map(LineHeightDeclarationValue::Value)
        .or_else(|| parse_line_height_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_line_height_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<LineHeightDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_line_height_property)
}

pub(in crate::native_engine::css) fn parse_opacity(value: &str) -> Option<u8> {
    let value = value.trim();
    let percentage = value.strip_suffix('%');
    let number = percentage.unwrap_or(value);
    let scaled = parse_decimal_milli(number)?;
    let denominator: u32 = if percentage.is_some() { 100_000 } else { 1_000 };
    if scaled > denominator {
        return None;
    }
    let alpha = (u64::from(scaled) * u64::from(u8::MAX) + u64::from(denominator / 2))
        / u64::from(denominator);
    u8::try_from(alpha).ok()
}

pub(in crate::native_engine::css) fn parse_opacity_custom_property(
    value: &str,
) -> Option<OpacityDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_opacity(fallback)
            .map(|fallback| OpacityDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(OpacityDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_opacity_property(
    value: &str,
) -> Option<OpacityDeclarationValue> {
    parse_opacity(value)
        .map(OpacityDeclarationValue::Value)
        .or_else(|| parse_opacity_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_opacity_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<OpacityDeclarationValue>> {
    parse_local_cascade_declaration(value, parse_opacity_property)
}

pub(in crate::native_engine::css) fn parse_decimal_milli(value: &str) -> Option<u32> {
    let (whole, fraction, has_decimal) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, fraction, true),
        None => (value, "", false),
    };
    if whole.is_empty() && !has_decimal || has_decimal && fraction.is_empty() {
        return None;
    }
    if !whole.is_empty() && !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let whole = if whole.is_empty() {
        0
    } else {
        whole.parse::<u32>().ok()?
    };
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u32>()
            .ok()?
            .saturating_mul(match fraction.len() {
                1 => 100,
                2 => 10,
                _ => 1,
            })
    };
    whole.checked_mul(1_000)?.checked_add(fraction)
}

/// Font-size accepts up to six fractional decimal digits; values are rounded
/// half-up to thousandths before the bounded resolver uses them. Keeping this
/// conversion local avoids widening unrelated CSS numeric parsers while
/// allowing CSS font-size declarations and calc factors to retain more input
/// precision than the native integer-pixel output model.
pub(in crate::native_engine::css) fn parse_font_size_decimal_milli(value: &str) -> Option<u32> {
    let (whole, fraction, has_decimal) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, fraction, true),
        None => (value, "", false),
    };
    if whole.is_empty() && !has_decimal || has_decimal && fraction.is_empty() {
        return None;
    }
    if !whole.is_empty() && !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if fraction.len() > 6 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let whole = if whole.is_empty() {
        0_u64
    } else {
        whole.parse::<u64>().ok()?
    };
    let fraction_len = fraction.len();
    let fraction = if fraction.is_empty() {
        0_u64
    } else {
        fraction.parse::<u64>().ok()?
    };
    let scale = 10_u64.checked_pow(u32::try_from(6_usize.saturating_sub(fraction_len)).ok()?)?;
    let fraction_micro = fraction.checked_mul(scale)?;
    let fraction_milli = fraction_micro.checked_add(500)?.checked_div(1_000)?;
    let milli = whole.checked_mul(1_000)?.checked_add(fraction_milli)?;
    u32::try_from(milli).ok()
}

pub(in crate::native_engine::css) fn parse_white_space(value: &str) -> Option<WhiteSpaceValue> {
    match value.to_ascii_lowercase().as_str() {
        "normal" => Some(WhiteSpaceValue::Normal),
        "pre-line" => Some(WhiteSpaceValue::PreLine),
        "pre" => Some(WhiteSpaceValue::Pre),
        "pre-wrap" => Some(WhiteSpaceValue::PreWrap),
        "nowrap" => Some(WhiteSpaceValue::NoWrap),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_white_space_custom_property(
    value: &str,
) -> Option<WhiteSpaceDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_white_space(fallback).map(|fallback| {
            WhiteSpaceDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(WhiteSpaceDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_white_space_property(
    value: &str,
) -> Option<WhiteSpaceDeclarationValue> {
    parse_white_space(value)
        .map(WhiteSpaceDeclarationValue::Value)
        .or_else(|| parse_white_space_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_white_space_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<WhiteSpaceDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_white_space_property)
}

pub(in crate::native_engine::css) fn parse_text_align(value: &str) -> Option<TextAlignValue> {
    match value.to_ascii_lowercase().as_str() {
        "left" => Some(TextAlignValue::Left),
        "center" => Some(TextAlignValue::Center),
        "right" => Some(TextAlignValue::Right),
        "start" => Some(TextAlignValue::Start),
        "end" => Some(TextAlignValue::End),
        "justify" => Some(TextAlignValue::Justify),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_text_align_custom_property(
    value: &str,
) -> Option<TextAlignDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_text_align(fallback)
            .map(|fallback| TextAlignDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(TextAlignDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_text_align_property(
    value: &str,
) -> Option<TextAlignDeclarationValue> {
    parse_text_align(value)
        .map(TextAlignDeclarationValue::Value)
        .or_else(|| parse_text_align_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_text_align_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextAlignDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_text_align_property)
}

pub(in crate::native_engine::css) fn parse_text_align_last(
    value: &str,
) -> Option<TextAlignLastValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(TextAlignLastValue::Auto),
        "left" => Some(TextAlignLastValue::Left),
        "center" => Some(TextAlignLastValue::Center),
        "right" => Some(TextAlignLastValue::Right),
        "start" => Some(TextAlignLastValue::Start),
        "end" => Some(TextAlignLastValue::End),
        "justify" => Some(TextAlignLastValue::Justify),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_text_align_last_custom_property(
    value: &str,
) -> Option<TextAlignLastDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_text_align_last(fallback).map(|fallback| {
            TextAlignLastDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(TextAlignLastDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_text_align_last_property(
    value: &str,
) -> Option<TextAlignLastDeclarationValue> {
    parse_text_align_last(value)
        .map(TextAlignLastDeclarationValue::Value)
        .or_else(|| parse_text_align_last_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_text_align_last_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextAlignLastDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_text_align_last_property)
}

pub(in crate::native_engine::css) fn parse_text_justify(value: &str) -> Option<TextJustifyValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(TextJustifyValue::Auto),
        "none" => Some(TextJustifyValue::None),
        "inter-word" => Some(TextJustifyValue::InterWord),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_text_justify_custom_property(
    value: &str,
) -> Option<TextJustifyDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_text_justify(fallback).map(|fallback| {
            TextJustifyDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(TextJustifyDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_text_justify_property(
    value: &str,
) -> Option<TextJustifyDeclarationValue> {
    parse_text_justify(value)
        .map(TextJustifyDeclarationValue::Value)
        .or_else(|| parse_text_justify_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_text_justify_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextJustifyDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_text_justify_property)
}

pub(in crate::native_engine::css) fn parse_justify_content(
    value: &str,
) -> Option<JustifyContentValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(JustifyContentValue::FlexStart),
        "normal" => Some(JustifyContentValue::Normal),
        "stretch" => Some(JustifyContentValue::Stretch),
        "center" => Some(JustifyContentValue::Center),
        "flex-end" => Some(JustifyContentValue::FlexEnd),
        "space-between" => Some(JustifyContentValue::SpaceBetween),
        "space-around" => Some(JustifyContentValue::SpaceAround),
        "space-evenly" => Some(JustifyContentValue::SpaceEvenly),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_justify_content_declaration(
    value: &str,
) -> Option<JustifyContentDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(JustifyContentDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(JustifyContentDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(JustifyContentDeclaration::Inherit);
    }
    parse_justify_content(value)
        .map(JustifyContentDeclaration::Value)
        .or_else(|| parse_justify_content_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_place_content(
    value: &str,
) -> Option<(AlignContentValue, JustifyContentValue)> {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    match values.as_slice() {
        [shared] => {
            let justify_content = parse_justify_content(shared)?;
            let align_content = parse_align_content(shared)?;
            Some((align_content, justify_content))
        }
        [align_content, justify_content] => Some((
            parse_align_content(align_content)?,
            parse_justify_content(justify_content)?,
        )),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_place_content_declaration(
    value: &str,
) -> Option<(AlignContentDeclaration, JustifyContentDeclaration)> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some((
            AlignContentDeclaration::RevertLayer,
            JustifyContentDeclaration::RevertLayer,
        ));
    }
    if is_local_reset_keyword(value) {
        return Some((
            AlignContentDeclaration::Reset,
            JustifyContentDeclaration::Reset,
        ));
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some((
            AlignContentDeclaration::Inherit,
            JustifyContentDeclaration::Inherit,
        ));
    }
    parse_place_content(value)
        .map(|(align_content, justify_content)| {
            (
                AlignContentDeclaration::Value(align_content),
                JustifyContentDeclaration::Value(justify_content),
            )
        })
        .or_else(|| parse_place_content_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_align_items(value: &str) -> Option<AlignItemsValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(AlignItemsValue::FlexStart),
        "center" => Some(AlignItemsValue::Center),
        "flex-end" => Some(AlignItemsValue::FlexEnd),
        "stretch" => Some(AlignItemsValue::Stretch),
        "normal" => Some(AlignItemsValue::Normal),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_align_items_declaration(
    value: &str,
) -> Option<AlignItemsDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(AlignItemsDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(AlignItemsDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(AlignItemsDeclaration::Inherit);
    }
    parse_align_items(value)
        .map(AlignItemsDeclaration::Value)
        .or_else(|| parse_align_items_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_align_self(value: &str) -> Option<AlignSelfValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(AlignSelfValue::Auto),
        "flex-start" => Some(AlignSelfValue::FlexStart),
        "center" => Some(AlignSelfValue::Center),
        "flex-end" => Some(AlignSelfValue::FlexEnd),
        "stretch" => Some(AlignSelfValue::Stretch),
        "normal" => Some(AlignSelfValue::Normal),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_align_self_declaration(
    value: &str,
) -> Option<AlignSelfDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(AlignSelfDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(AlignSelfDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(AlignSelfDeclaration::Inherit);
    }
    parse_align_self(value)
        .map(AlignSelfDeclaration::Value)
        .or_else(|| parse_align_self_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_align_content(value: &str) -> Option<AlignContentValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(AlignContentValue::FlexStart),
        "center" => Some(AlignContentValue::Center),
        "flex-end" => Some(AlignContentValue::FlexEnd),
        "space-between" => Some(AlignContentValue::SpaceBetween),
        "space-around" => Some(AlignContentValue::SpaceAround),
        "space-evenly" => Some(AlignContentValue::SpaceEvenly),
        "stretch" => Some(AlignContentValue::Stretch),
        "normal" => Some(AlignContentValue::Normal),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_align_content_declaration(
    value: &str,
) -> Option<AlignContentDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(AlignContentDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(AlignContentDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(AlignContentDeclaration::Inherit);
    }
    parse_align_content(value)
        .map(AlignContentDeclaration::Value)
        .or_else(|| parse_align_content_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_var_arguments(
    value: &str,
) -> Option<(u64, Option<&str>)> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    Some((parse_custom_property_name(name)?, fallback))
}
pub(in crate::native_engine::css) fn parse_justify_content_custom_property(
    value: &str,
) -> Option<JustifyContentDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_justify_content(fallback)
            .map(|fallback| JustifyContentDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(JustifyContentDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_align_items_custom_property(
    value: &str,
) -> Option<AlignItemsDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_align_items(fallback)
            .map(|fallback| AlignItemsDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(AlignItemsDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_align_self_custom_property(
    value: &str,
) -> Option<AlignSelfDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_align_self(fallback)
            .map(|fallback| AlignSelfDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(AlignSelfDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_align_content_custom_property(
    value: &str,
) -> Option<AlignContentDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_align_content(fallback)
            .map(|fallback| AlignContentDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(AlignContentDeclaration::CustomProperty(name_hash)),
    }
}
pub(in crate::native_engine::css) fn parse_place_content_custom_property(
    value: &str,
) -> Option<(AlignContentDeclaration, JustifyContentDeclaration)> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => {
            let (align_content, justify_content) = parse_place_content(fallback)?;
            Some((
                AlignContentDeclaration::CustomPropertyPlaceContentFallback(
                    name_hash,
                    align_content,
                ),
                JustifyContentDeclaration::CustomPropertyPlaceContentFallback(
                    name_hash,
                    justify_content,
                ),
            ))
        }
        None => Some((
            AlignContentDeclaration::CustomPropertyPlaceContent(name_hash),
            JustifyContentDeclaration::CustomPropertyPlaceContent(name_hash),
        )),
    }
}

pub(in crate::native_engine::css) fn parse_flex_direction_custom_property(
    value: &str,
) -> Option<FlexDirectionDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_flex_direction(fallback)
            .map(|fallback| FlexDirectionDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FlexDirectionDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_flex_wrap_custom_property(
    value: &str,
) -> Option<FlexWrapDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_flex_wrap(fallback)
            .map(|fallback| FlexWrapDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FlexWrapDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_flex_flow_custom_property(
    value: &str,
) -> Option<(FlexDirectionDeclaration, FlexWrapDeclaration)> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => {
            let (direction, wrap) = parse_flex_flow(fallback)?;
            Some((
                FlexDirectionDeclaration::CustomPropertyFlowFallback(name_hash, direction),
                FlexWrapDeclaration::CustomPropertyFlowFallback(name_hash, wrap),
            ))
        }
        None => Some((
            FlexDirectionDeclaration::CustomPropertyFlow(name_hash),
            FlexWrapDeclaration::CustomPropertyFlow(name_hash),
        )),
    }
}

pub(in crate::native_engine::css) fn parse_flex_direction(
    value: &str,
) -> Option<FlexDirectionValue> {
    match value.to_ascii_lowercase().as_str() {
        "row" => Some(FlexDirectionValue::Row),
        "row-reverse" => Some(FlexDirectionValue::RowReverse),
        "column" => Some(FlexDirectionValue::Column),
        "column-reverse" => Some(FlexDirectionValue::ColumnReverse),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_flex_direction_declaration(
    value: &str,
) -> Option<FlexDirectionDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(FlexDirectionDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(FlexDirectionDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(FlexDirectionDeclaration::Inherit);
    }
    parse_flex_direction(value)
        .map(FlexDirectionDeclaration::Value)
        .or_else(|| parse_flex_direction_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_direction(value: &str) -> Option<DirectionValue> {
    match value.to_ascii_lowercase().as_str() {
        "ltr" => Some(DirectionValue::Ltr),
        "rtl" => Some(DirectionValue::Rtl),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_direction_custom_property(
    value: &str,
) -> Option<DirectionDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_direction(fallback)
            .map(|fallback| DirectionDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(DirectionDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_direction_property(
    value: &str,
) -> Option<DirectionDeclarationValue> {
    parse_direction(value)
        .map(DirectionDeclarationValue::Value)
        .or_else(|| parse_direction_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_direction_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<DirectionDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_direction_property)
}

pub(in crate::native_engine::css) fn parse_flex_wrap(value: &str) -> Option<FlexWrapValue> {
    match value.to_ascii_lowercase().as_str() {
        "nowrap" => Some(FlexWrapValue::NoWrap),
        "wrap" => Some(FlexWrapValue::Wrap),
        "wrap-reverse" => Some(FlexWrapValue::WrapReverse),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_flex_wrap_declaration(
    value: &str,
) -> Option<FlexWrapDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(FlexWrapDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(FlexWrapDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(FlexWrapDeclaration::Inherit);
    }
    parse_flex_wrap(value)
        .map(FlexWrapDeclaration::Value)
        .or_else(|| parse_flex_wrap_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_flow(
    value: &str,
) -> Option<(FlexDirectionValue, FlexWrapValue)> {
    let mut direction = None;
    let mut wrap = None;
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    if values.is_empty() || values.len() > 2 {
        return None;
    }
    for value in values {
        if let Some(parsed) = parse_flex_direction(value) {
            if direction.replace(parsed).is_some() {
                return None;
            }
        } else {
            let parsed = parse_flex_wrap(value)?;
            if wrap.replace(parsed).is_some() {
                return None;
            }
        }
    }
    Some((direction.unwrap_or_default(), wrap.unwrap_or_default()))
}

pub(in crate::native_engine::css) fn parse_flex_flow_declaration(
    value: &str,
) -> Option<(FlexDirectionDeclaration, FlexWrapDeclaration)> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some((
            FlexDirectionDeclaration::RevertLayer,
            FlexWrapDeclaration::RevertLayer,
        ));
    }
    if is_local_reset_keyword(value) {
        return Some((FlexDirectionDeclaration::Reset, FlexWrapDeclaration::Reset));
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some((
            FlexDirectionDeclaration::Inherit,
            FlexWrapDeclaration::Inherit,
        ));
    }
    parse_flex_flow(value)
        .map(|(direction, wrap)| {
            (
                FlexDirectionDeclaration::Value(direction),
                FlexWrapDeclaration::Value(wrap),
            )
        })
        .or_else(|| parse_flex_flow_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_item_order(
    value: &str,
) -> Option<NativeOrderValue> {
    let value = value.trim();
    let digits = match value.as_bytes().first() {
        Some(b'+') | Some(b'-') => &value[1..],
        _ => value,
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<i32>().ok()?;
    (MIN_NATIVE_FLEX_ITEM_ORDER..=MAX_NATIVE_FLEX_ITEM_ORDER)
        .contains(&parsed)
        .then_some(NativeOrderValue(parsed))
}

pub(in crate::native_engine::css) fn parse_flex_item_order_custom_property(
    value: &str,
) -> Option<FlexItemOrderDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_flex_item_order(fallback)
            .map(|fallback| FlexItemOrderDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FlexItemOrderDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_flex_item_order_declaration(
    value: &str,
) -> Option<FlexItemOrderDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(FlexItemOrderDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(FlexItemOrderDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(FlexItemOrderDeclaration::Inherit);
    }
    parse_flex_item_order(value)
        .map(FlexItemOrderDeclaration::Value)
        .or_else(|| parse_flex_item_order_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_grow(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    (0..=MAX_NATIVE_FLEX_GROW)
        .contains(&parsed)
        .then_some(parsed)
}

pub(in crate::native_engine::css) fn parse_flex_grow_custom_property(
    value: &str,
) -> Option<FlexGrowDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_flex_grow(fallback)
            .map(|fallback| FlexGrowDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FlexGrowDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_flex_shrink_custom_property(
    value: &str,
) -> Option<FlexShrinkDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_flex_shrink(fallback)
            .map(|fallback| FlexShrinkDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FlexShrinkDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_flex_basis_custom_property(
    value: &str,
) -> Option<FlexBasisDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_flex_basis(fallback)
            .map(|fallback| FlexBasisDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FlexBasisDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_flex_grow_declaration(
    value: &str,
) -> Option<FlexGrowDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(FlexGrowDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(FlexGrowDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(FlexGrowDeclaration::Inherit);
    }
    parse_flex_grow(value)
        .map(FlexGrowDeclaration::Value)
        .or_else(|| parse_flex_grow_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_shrink(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    (0..=MAX_NATIVE_FLEX_SHRINK)
        .contains(&parsed)
        .then_some(parsed)
}

pub(in crate::native_engine::css) fn parse_flex_shrink_declaration(
    value: &str,
) -> Option<FlexShrinkDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(FlexShrinkDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(FlexShrinkDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(FlexShrinkDeclaration::Inherit);
    }
    parse_flex_shrink(value)
        .map(FlexShrinkDeclaration::Value)
        .or_else(|| parse_flex_shrink_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_basis(value: &str) -> Option<FlexBasisValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Some(FlexBasisValue::Auto);
    }
    parse_dimension(value).map(FlexBasisValue::Length)
}

pub(in crate::native_engine::css) fn parse_flex_basis_declaration(
    value: &str,
) -> Option<FlexBasisDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(FlexBasisDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(FlexBasisDeclaration::Reset);
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some(FlexBasisDeclaration::Inherit);
    }
    parse_flex_basis(value)
        .map(FlexBasisDeclaration::Value)
        .or_else(|| parse_flex_basis_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_flex_shorthand(
    value: &str,
) -> Option<(u32, u32, FlexBasisValue)> {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    match values.as_slice() {
        [value] if value.eq_ignore_ascii_case("none") => Some((0, 0, FlexBasisValue::Auto)),
        [value] if value.eq_ignore_ascii_case("auto") => Some((1, 1, FlexBasisValue::Auto)),
        [grow] => Some((parse_flex_grow(grow)?, 1, FlexBasisValue::Length(0))),
        [grow, second] => {
            let grow = parse_flex_grow(grow)?;
            if let Some(shrink) = parse_flex_shrink(second) {
                Some((grow, shrink, FlexBasisValue::Length(0)))
            } else {
                Some((grow, 1, parse_flex_basis(second)?))
            }
        }
        [grow, shrink, basis] => Some((
            parse_flex_grow(grow)?,
            parse_flex_shrink(shrink)?,
            parse_flex_basis(basis)?,
        )),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_flex_shorthand_custom_property(
    value: &str,
) -> Option<(
    FlexGrowDeclaration,
    FlexShrinkDeclaration,
    FlexBasisDeclaration,
)> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => {
            let (grow, shrink, basis) = parse_flex_shorthand(fallback)?;
            Some((
                FlexGrowDeclaration::CustomPropertyShorthandFallback(name_hash, grow),
                FlexShrinkDeclaration::CustomPropertyShorthandFallback(name_hash, shrink),
                FlexBasisDeclaration::CustomPropertyShorthandFallback(name_hash, basis),
            ))
        }
        None => Some((
            FlexGrowDeclaration::CustomPropertyShorthand(name_hash),
            FlexShrinkDeclaration::CustomPropertyShorthand(name_hash),
            FlexBasisDeclaration::CustomPropertyShorthand(name_hash),
        )),
    }
}

pub(in crate::native_engine::css) fn parse_flex_shorthand_declaration(
    value: &str,
) -> Option<(
    FlexGrowDeclaration,
    FlexShrinkDeclaration,
    FlexBasisDeclaration,
)> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some((
            FlexGrowDeclaration::RevertLayer,
            FlexShrinkDeclaration::RevertLayer,
            FlexBasisDeclaration::RevertLayer,
        ));
    }
    if is_local_reset_keyword(value) {
        return Some((
            FlexGrowDeclaration::Reset,
            FlexShrinkDeclaration::Reset,
            FlexBasisDeclaration::Reset,
        ));
    }
    if value.trim().eq_ignore_ascii_case("inherit") {
        return Some((
            FlexGrowDeclaration::Inherit,
            FlexShrinkDeclaration::Inherit,
            FlexBasisDeclaration::Inherit,
        ));
    }
    parse_flex_shorthand(value)
        .map(|(grow, shrink, basis)| {
            (
                FlexGrowDeclaration::Value(grow),
                FlexShrinkDeclaration::Value(shrink),
                FlexBasisDeclaration::Value(basis),
            )
        })
        .or_else(|| parse_flex_shorthand_custom_property(value))
}
