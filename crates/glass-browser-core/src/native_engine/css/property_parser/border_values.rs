use super::*;

pub(in crate::native_engine::css) fn parse_border_radius(
    value: &str,
) -> Option<NativeBorderRadius> {
    if value.contains('/') {
        return None;
    }
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some(NativeBorderRadius {
            top_left: *all,
            top_right: *all,
            bottom_right: *all,
            bottom_left: *all,
        }),
        [top_left, top_right] => Some(NativeBorderRadius {
            top_left: *top_left,
            top_right: *top_right,
            bottom_right: *top_left,
            bottom_left: *top_right,
        }),
        [top_left, top_right, bottom_right] => Some(NativeBorderRadius {
            top_left: *top_left,
            top_right: *top_right,
            bottom_right: *bottom_right,
            bottom_left: *top_right,
        }),
        [top_left, top_right, bottom_right, bottom_left] => Some(NativeBorderRadius {
            top_left: *top_left,
            top_right: *top_right,
            bottom_right: *bottom_right,
            bottom_left: *bottom_left,
        }),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_border_radius_var_arguments(
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

pub(in crate::native_engine::css) fn parse_border_radius_custom_property(
    value: &str,
) -> Option<NativeBorderRadiusValue> {
    let (name_hash, fallback) = parse_border_radius_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_border_radius(fallback)
            .map(|fallback| NativeBorderRadiusValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(NativeBorderRadiusValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_border_radius_corner_custom_property(
    value: &str,
) -> Option<NativeBorderRadiusValue> {
    let (name_hash, fallback) = parse_border_radius_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_dimension(fallback).map(|fallback| {
            NativeBorderRadiusValue::CustomPropertyCornerFallback(name_hash, fallback)
        }),
        None => Some(NativeBorderRadiusValue::CustomPropertyCorner(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_border_radius_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderRadiusValue>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    let css_wide = if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderRadiusValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderRadiusValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderRadiusValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderRadiusValue::Revert)
    } else {
        None
    };
    css_wide
        .or_else(|| parse_border_radius(value).map(NativeBorderRadiusValue::Radius))
        .or_else(|| parse_border_radius_custom_property(value))
        .map(LocalCascadeDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_border_radius_corner_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderRadiusValue>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    let css_wide = if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderRadiusValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderRadiusValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderRadiusValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderRadiusValue::Revert)
    } else {
        None
    };
    css_wide
        .or_else(|| parse_dimension(value).map(NativeBorderRadiusValue::Corner))
        .or_else(|| parse_border_radius_corner_custom_property(value))
        .map(LocalCascadeDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_box_edges(value: &str) -> Option<[u32; 4]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    expand_box_edges(&values)
}

pub(in crate::native_engine::css) fn parse_local_box_edges(
    value: &str,
) -> Option<[LocalCascadeDeclaration<PaddingDeclarationValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 4]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(PaddingDeclarationValue::Value(0)); 4]);
    }
    if let Some(values) = parse_box_edges(value) {
        return Some(
            values
                .map(|value| LocalCascadeDeclaration::Value(PaddingDeclarationValue::Value(value))),
        );
    }
    let (name_hash, fallback) = parse_padding_shorthand_custom_property(value)?;
    Some(std::array::from_fn(|index| {
        LocalCascadeDeclaration::Value(match fallback {
            Some(values) => PaddingDeclarationValue::CustomPropertyShorthandFallback(
                name_hash,
                index as u8,
                values[index],
            ),
            None => PaddingDeclarationValue::CustomPropertyShorthand(name_hash, index as u8),
        })
    }))
}

pub(in crate::native_engine::css) fn parse_box_edge_pair(value: &str) -> Option<[u32; 2]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some([*all; 2]),
        [start, end] => Some([*start, *end]),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_local_box_edge_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<u32>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 2]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(0); 2]);
    }
    parse_box_edge_pair(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

pub(in crate::native_engine::css) fn parse_margin_value(value: &str) -> Option<NativeMarginValue> {
    if value.eq_ignore_ascii_case("auto") {
        Some(NativeMarginValue::Auto)
    } else {
        parse_dimension(value).map(NativeMarginValue::Length)
    }
}

pub(in crate::native_engine::css) fn parse_margin_edges(
    value: &str,
) -> Option<[NativeMarginValue; 4]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_margin_value)
        .collect::<Option<Vec<_>>>()?;
    expand_box_edges(&values)
}

pub(in crate::native_engine::css) fn parse_local_margin_edges(
    value: &str,
) -> Option<[LocalCascadeDeclaration<MarginDeclarationValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 4]);
    }
    if is_local_reset_keyword(value) {
        return Some(
            [LocalCascadeDeclaration::Value(MarginDeclarationValue::Value(
                NativeMarginValue::Length(0),
            )); 4],
        );
    }
    if let Some(values) = parse_margin_edges(value) {
        return Some(
            values
                .map(|value| LocalCascadeDeclaration::Value(MarginDeclarationValue::Value(value))),
        );
    }
    let (name_hash, fallback) = parse_margin_shorthand_custom_property(value)?;
    Some(std::array::from_fn(|index| {
        LocalCascadeDeclaration::Value(match fallback {
            Some(values) => MarginDeclarationValue::CustomPropertyShorthandFallback(
                name_hash,
                index as u8,
                values[index],
            ),
            None => MarginDeclarationValue::CustomPropertyShorthand(name_hash, index as u8),
        })
    }))
}

pub(in crate::native_engine::css) fn parse_margin_edge_pair(
    value: &str,
) -> Option<[NativeMarginValue; 2]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_margin_value)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some([*all; 2]),
        [start, end] => Some([*start, *end]),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_local_margin_edge_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeMarginValue>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 2]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)); 2]);
    }
    parse_margin_edge_pair(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

pub(in crate::native_engine::css) fn expand_box_edges<T: Copy>(values: &[T]) -> Option<[T; 4]> {
    match values {
        [all] => Some([*all; 4]),
        [vertical, horizontal] => Some([*vertical, *horizontal, *vertical, *horizontal]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_border_style(value: &str) -> Option<NativeBorderStyle> {
    match value.to_ascii_lowercase().as_str() {
        "solid" => Some(NativeBorderStyle::Solid),
        "dashed" => Some(NativeBorderStyle::Dashed),
        "dotted" => Some(NativeBorderStyle::Dotted),
        "double" => Some(NativeBorderStyle::Double),
        "groove" => Some(NativeBorderStyle::Groove),
        "ridge" => Some(NativeBorderStyle::Ridge),
        "inset" => Some(NativeBorderStyle::Inset),
        "outset" => Some(NativeBorderStyle::Outset),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_border_style_value(
    value: &str,
) -> Option<NativeBorderStyleValue> {
    if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderStyleValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderStyleValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderStyleValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderStyleValue::Revert)
    } else if value.eq_ignore_ascii_case("none") {
        Some(NativeBorderStyleValue::None)
    } else if value.eq_ignore_ascii_case("hidden") {
        Some(NativeBorderStyleValue::Hidden)
    } else {
        parse_border_style(value).map(NativeBorderStyleValue::Paint)
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_style_value(
    value: &str,
) -> Option<NativeTextDecorationStyle> {
    match value.to_ascii_lowercase().as_str() {
        "solid" => Some(NativeTextDecorationStyle::Solid),
        "dashed" => Some(NativeTextDecorationStyle::Dashed),
        "dotted" => Some(NativeTextDecorationStyle::Dotted),
        "double" => Some(NativeTextDecorationStyle::Double),
        "wavy" => Some(NativeTextDecorationStyle::Wavy),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_style(
    value: &str,
) -> Option<NativeTextDecorationStyleDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_text_decoration_style_value(fallback).map(|fallback| {
                NativeTextDecorationStyleDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeTextDecorationStyleDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    match value.to_ascii_lowercase().as_str() {
        "inherit" => Some(NativeTextDecorationStyleDeclaration::Inherit),
        "initial" => Some(NativeTextDecorationStyleDeclaration::Initial),
        "unset" => Some(NativeTextDecorationStyleDeclaration::Unset),
        "revert" => Some(NativeTextDecorationStyleDeclaration::Revert),
        "revert-layer" => Some(NativeTextDecorationStyleDeclaration::RevertLayer),
        _ => parse_text_decoration_style_value(value)
            .map(NativeTextDecorationStyleDeclaration::Value),
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_skip_ink_value(
    value: &str,
) -> Option<NativeTextDecorationSkipInk> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(NativeTextDecorationSkipInk::Auto),
        "none" => Some(NativeTextDecorationSkipInk::None),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_skip_ink(
    value: &str,
) -> Option<NativeTextDecorationSkipInkDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_text_decoration_skip_ink_value(fallback).map(|fallback| {
                NativeTextDecorationSkipInkDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeTextDecorationSkipInkDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    match value.to_ascii_lowercase().as_str() {
        "inherit" => Some(NativeTextDecorationSkipInkDeclaration::Inherit),
        "initial" => Some(NativeTextDecorationSkipInkDeclaration::Initial),
        "unset" => Some(NativeTextDecorationSkipInkDeclaration::Unset),
        "revert" => Some(NativeTextDecorationSkipInkDeclaration::Revert),
        "revert-layer" => Some(NativeTextDecorationSkipInkDeclaration::RevertLayer),
        _ => parse_text_decoration_skip_ink_value(value)
            .map(NativeTextDecorationSkipInkDeclaration::Value),
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_skip_spaces_value(
    value: &str,
) -> Option<NativeTextDecorationSkipSpaces> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    match tokens.as_slice() {
        [token] => match token.to_ascii_lowercase().as_str() {
            "none" => Some(NativeTextDecorationSkipSpaces::None),
            "all" => Some(NativeTextDecorationSkipSpaces::All),
            "start" => Some(NativeTextDecorationSkipSpaces::Start),
            "end" => Some(NativeTextDecorationSkipSpaces::End),
            _ => None,
        },
        [first, second] => {
            let first = first.to_ascii_lowercase();
            let second = second.to_ascii_lowercase();
            if matches!(first.as_str(), "start" | "end")
                && matches!(second.as_str(), "start" | "end")
                && first != second
            {
                Some(NativeTextDecorationSkipSpaces::StartAndEnd)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_skip_spaces(
    value: &str,
) -> Option<NativeTextDecorationSkipSpacesDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_text_decoration_skip_spaces_value(fallback).map(|fallback| {
                NativeTextDecorationSkipSpacesDeclaration::CustomPropertyFallback(
                    name_hash, fallback,
                )
            }),
            None => Some(NativeTextDecorationSkipSpacesDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    match tokens.as_slice() {
        [token] => match token.to_ascii_lowercase().as_str() {
            "initial" => Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd,
            )),
            "inherit" => Some(NativeTextDecorationSkipSpacesDeclaration::Inherit),
            "unset" => Some(NativeTextDecorationSkipSpacesDeclaration::Unset),
            "revert" => Some(NativeTextDecorationSkipSpacesDeclaration::Revert),
            "revert-layer" => Some(NativeTextDecorationSkipSpacesDeclaration::RevertLayer),
            _ => parse_text_decoration_skip_spaces_value(value)
                .map(NativeTextDecorationSkipSpacesDeclaration::Value),
        },
        _ => parse_text_decoration_skip_spaces_value(value)
            .map(NativeTextDecorationSkipSpacesDeclaration::Value),
    }
}

pub(in crate::native_engine::css) fn parse_text_decoration_thickness_value(
    value: &str,
) -> Option<u32> {
    parse_dimension(value)
        .filter(|value| (1..=MAX_NATIVE_TEXT_DECORATION_THICKNESS).contains(value))
}

pub(in crate::native_engine::css) fn parse_text_decoration_thickness(
    value: &str,
) -> Option<NativeTextDecorationThicknessDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_text_decoration_thickness_value(fallback).map(|fallback| {
                NativeTextDecorationThicknessDeclaration::CustomPropertyFallback(
                    name_hash, fallback,
                )
            }),
            None => Some(NativeTextDecorationThicknessDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    if value.eq_ignore_ascii_case("inherit") {
        return Some(NativeTextDecorationThicknessDeclaration::Inherit);
    }
    if value.eq_ignore_ascii_case("initial") {
        return Some(NativeTextDecorationThicknessDeclaration::Initial);
    }
    if value.eq_ignore_ascii_case("unset") {
        return Some(NativeTextDecorationThicknessDeclaration::Unset);
    }
    if value.eq_ignore_ascii_case("revert") {
        return Some(NativeTextDecorationThicknessDeclaration::Revert);
    }
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationThicknessDeclaration::RevertLayer);
    }
    parse_text_decoration_thickness_value(value)
        .map(NativeTextDecorationThicknessDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_text_underline_offset_value(value: &str) -> Option<i32> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_suffix("px")?.trim();
    let (negative, magnitude) = match value.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, value),
    };
    if magnitude.is_empty() || !magnitude.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let magnitude = magnitude.parse::<i32>().ok()?;
    if magnitude > MAX_NATIVE_TEXT_UNDERLINE_OFFSET {
        return None;
    }
    Some(if negative { -magnitude } else { magnitude })
}

pub(in crate::native_engine::css) fn parse_text_underline_offset(
    value: &str,
) -> Option<NativeTextUnderlineOffsetDeclaration> {
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(value) {
        return match fallback {
            Some(fallback) => parse_text_underline_offset_value(fallback).map(|fallback| {
                NativeTextUnderlineOffsetDeclaration::CustomPropertyFallback(name_hash, fallback)
            }),
            None => Some(NativeTextUnderlineOffsetDeclaration::CustomProperty(
                name_hash,
            )),
        };
    }
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "inherit" => return Some(NativeTextUnderlineOffsetDeclaration::Inherit),
        "initial" => return Some(NativeTextUnderlineOffsetDeclaration::Initial),
        "unset" => return Some(NativeTextUnderlineOffsetDeclaration::Unset),
        "revert" => return Some(NativeTextUnderlineOffsetDeclaration::Revert),
        "revert-layer" => return Some(NativeTextUnderlineOffsetDeclaration::RevertLayer),
        _ => {}
    }
    parse_text_underline_offset_value(value).map(NativeTextUnderlineOffsetDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_text_decoration_color(
    value: &str,
) -> Option<NativeTextDecorationColorDeclaration> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationColorDeclaration::RevertLayer);
    }
    if value.eq_ignore_ascii_case("currentColor") {
        return Some(NativeTextDecorationColorDeclaration::CurrentColor);
    }
    for (keyword, declaration) in [
        ("inherit", NativeTextDecorationColorDeclaration::Inherit),
        ("unset", NativeTextDecorationColorDeclaration::Unset),
        ("initial", NativeTextDecorationColorDeclaration::Initial),
        ("revert", NativeTextDecorationColorDeclaration::Revert),
    ] {
        if value.eq_ignore_ascii_case(keyword) {
            return Some(declaration);
        }
    }
    if let Some(value) = parse_color_custom_property(value) {
        match value {
            NativeColorValue::Color(color) => {
                Some(NativeTextDecorationColorDeclaration::Value(color))
            }
            NativeColorValue::CurrentColor => {
                Some(NativeTextDecorationColorDeclaration::CurrentColor)
            }
            NativeColorValue::Inherit => Some(NativeTextDecorationColorDeclaration::Inherit),
            NativeColorValue::Unset => Some(NativeTextDecorationColorDeclaration::Unset),
            NativeColorValue::Initial => Some(NativeTextDecorationColorDeclaration::Initial),
            NativeColorValue::Revert => Some(NativeTextDecorationColorDeclaration::Revert),
            NativeColorValue::CustomProperty(name_hash) => Some(
                NativeTextDecorationColorDeclaration::CustomProperty(name_hash),
            ),
            NativeColorValue::CustomPropertyFallback(name_hash, fallback) => Some(
                NativeTextDecorationColorDeclaration::CustomPropertyFallback(name_hash, fallback),
            ),
        }
    } else {
        parse_color(value).map(NativeTextDecorationColorDeclaration::Value)
    }
}

pub(in crate::native_engine::css) fn parse_color_custom_property(
    value: &str,
) -> Option<NativeColorValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_color(fallback)
            .map(|fallback| NativeColorValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(NativeColorValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_local_color_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeColorValue>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    let keyword = if value.eq_ignore_ascii_case("currentColor") {
        Some(NativeColorValue::CurrentColor)
    } else if value.eq_ignore_ascii_case("inherit") {
        Some(NativeColorValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeColorValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeColorValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeColorValue::Revert)
    } else {
        None
    };
    keyword
        .or_else(|| parse_color(value).map(NativeColorValue::Color))
        .or_else(|| parse_color_custom_property(value))
        .map(LocalCascadeDeclaration::Value)
}

pub(in crate::native_engine::css) fn set_border_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(border) = parse_border_declaration(value) {
        sides[index] = Some(border);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_border_radius_corner(
    corners: &mut [Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(radius) = parse_border_radius_corner_declaration(value) {
        corners[index] = Some(radius);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_logical_border_pair<T: Copy>(
    sides: &mut [Option<LocalCascadeDeclaration<T>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<T>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

pub(in crate::native_engine::css) fn set_logical_border_width_pair(
    sides: &mut [Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>>;
             LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<BorderWidthDeclarationValue>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

pub(in crate::native_engine::css) fn set_logical_border_width_side(
    sides: &mut [Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>>;
             LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_logical_border_style_pair(
    sides: &mut [Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>>;
             LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<BorderStyleDeclarationValue>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

pub(in crate::native_engine::css) fn set_logical_border_style_side(
    sides: &mut [Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>>;
             LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_logical_border_side<T: Copy>(
    sides: &mut [Option<LocalCascadeDeclaration<T>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<T>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_logical_border_color_pair(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

pub(in crate::native_engine::css) fn set_logical_border_color_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<NativeBorderColorValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_logical_border_radius_corner(
    corners: &mut [Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        corners[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_border_color_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(color) = parse_border_color_side_declaration(value) {
        sides[index] = Some(color);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_border_width_side(
    sides: &mut [Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(width) = parse_local_physical_border_width_side_declaration(value) {
        sides[index] = Some(width);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

pub(in crate::native_engine::css) fn set_border_style_side(
    sides: &mut [Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(style) = parse_local_physical_border_style_side_declaration(value) {
        sides[index] = Some(style);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}
