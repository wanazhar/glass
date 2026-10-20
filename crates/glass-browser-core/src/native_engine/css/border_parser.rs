use super::*;

#[cfg(test)]
pub(super) fn parse_border(value: &str) -> Option<NativeBorderSide> {
    let mut parts = value.split_ascii_whitespace();
    let width = parse_dimension(parts.next()?)?;
    let style = parse_border_style(parts.next()?)?;
    let color = parts.collect::<Vec<_>>().join(" ");
    Some(NativeBorderSide {
        width,
        style,
        color: parse_color(&color)?,
    })
}

pub(super) fn parse_complete_border(value: &str) -> Option<NativeBorderDeclaration> {
    let mut parts = value.split_ascii_whitespace();
    let width = parse_dimension(parts.next()?)?;
    let style = parts.next()?;
    let color = parts.collect::<Vec<_>>().join(" ");
    let color = parse_border_color_value(&color)?;
    match (parse_border_style_value(style)?, color) {
        (NativeBorderStyleValue::Paint(style), NativeBorderColorValue::Color(color)) => {
            Some(NativeBorderDeclaration::Complete(NativeBorderSide {
                width,
                style,
                color,
            }))
        }
        (NativeBorderStyleValue::Paint(style), NativeBorderColorValue::CurrentColor) => {
            Some(NativeBorderDeclaration::CompleteCurrentColor { width, style })
        }
        (NativeBorderStyleValue::None, NativeBorderColorValue::Color(color)) => {
            Some(NativeBorderDeclaration::CompleteNone { width, color })
        }
        (NativeBorderStyleValue::None, NativeBorderColorValue::CurrentColor) => {
            Some(NativeBorderDeclaration::CompleteNoneCurrentColor { width })
        }
        (NativeBorderStyleValue::Hidden, NativeBorderColorValue::Color(color)) => {
            Some(NativeBorderDeclaration::CompleteHidden { width, color })
        }
        (NativeBorderStyleValue::Hidden, NativeBorderColorValue::CurrentColor) => {
            Some(NativeBorderDeclaration::CompleteHiddenCurrentColor { width })
        }
        _ => None,
    }
}

pub(super) fn parse_border_var_arguments(value: &str) -> Option<(u64, Option<&str>)> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    Some((parse_custom_property_name(name)?, fallback))
}

pub(super) fn parse_border_custom_property(value: &str) -> Option<NativeBorderDeclaration> {
    let (name_hash, fallback) = parse_border_var_arguments(value)?;
    match fallback {
        Some(fallback) => match parse_complete_border(fallback)? {
            NativeBorderDeclaration::Complete(fallback) => Some(
                NativeBorderDeclaration::CustomPropertyFallback(name_hash, fallback),
            ),
            _ => None,
        },
        None => Some(NativeBorderDeclaration::CustomProperty(name_hash)),
    }
}

pub(super) fn resolve_border_declaration_value(
    declaration: NativeBorderDeclaration,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeBorderDeclaration> {
    if depth >= MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH {
        return None;
    }
    match declaration {
        NativeBorderDeclaration::CustomProperty(name_hash) => custom_properties
            .get(&name_hash)
            .and_then(|value| parse_border_declaration(value))
            .and_then(|declaration| match declaration {
                LocalCascadeDeclaration::Value(value) => {
                    resolve_border_declaration_value(value, custom_properties, depth + 1)
                }
                LocalCascadeDeclaration::Inherit
                | LocalCascadeDeclaration::Reset
                | LocalCascadeDeclaration::RevertLayer => None,
            }),
        NativeBorderDeclaration::CustomPropertyFallback(name_hash, fallback) => custom_properties
            .get(&name_hash)
            .and_then(|value| parse_border_declaration(value))
            .and_then(|declaration| match declaration {
                LocalCascadeDeclaration::Value(value) => {
                    resolve_border_declaration_value(value, custom_properties, depth + 1)
                }
                LocalCascadeDeclaration::Inherit
                | LocalCascadeDeclaration::Reset
                | LocalCascadeDeclaration::RevertLayer => None,
            })
            .or(Some(NativeBorderDeclaration::Complete(fallback))),
        value => Some(value),
    }
}

pub(super) fn resolve_local_border_declaration<const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderDeclaration>>>; N],
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeBorderDeclaration> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(resolve_border_declaration_value(
            value,
            custom_properties,
            0,
        )),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(Some(NativeBorderDeclaration::None)),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

pub(super) fn parse_border_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderDeclaration>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    if value.eq_ignore_ascii_case("none") {
        return Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None,
        ));
    }
    if value.eq_ignore_ascii_case("hidden") {
        return Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Hidden,
        ));
    }
    for (keyword, declaration) in [
        ("inherit", NativeBorderDeclaration::Inherit),
        ("unset", NativeBorderDeclaration::Unset),
        ("initial", NativeBorderDeclaration::Initial),
        ("revert", NativeBorderDeclaration::Revert),
    ] {
        if value.eq_ignore_ascii_case(keyword) {
            return Some(LocalCascadeDeclaration::Value(declaration));
        }
    }
    parse_complete_border(value)
        .or_else(|| parse_border_custom_property(value))
        .map(LocalCascadeDeclaration::Value)
}

pub(super) fn parse_border_width_edges(value: &str) -> Option<[NativeBorderWidthValue; 4]> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_width_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    expand_box_edges(&values)
}

pub(super) fn parse_border_width_var_arguments(value: &str) -> Option<(u64, Option<&str>)> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    Some((parse_custom_property_name(name)?, fallback))
}

pub(super) fn parse_border_width_custom_property(
    value: &str,
) -> Option<BorderWidthDeclarationValue> {
    let (name_hash, fallback) = parse_border_width_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_border_width_value(fallback).map(|fallback| {
            BorderWidthDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(BorderWidthDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(super) fn parse_border_width_shorthand_custom_property(
    value: &str,
) -> Option<(u64, Option<[NativeBorderWidthValue; 4]>)> {
    let (name_hash, fallback) = parse_border_width_var_arguments(value)?;
    let fallback = match fallback {
        Some(fallback) => Some(parse_border_width_edges(fallback)?),
        None => None,
    };
    Some((name_hash, fallback))
}

pub(super) fn parse_border_width_property(value: &str) -> Option<BorderWidthDeclarationValue> {
    parse_border_width_value(value)
        .map(BorderWidthDeclarationValue::Value)
        .or_else(|| parse_border_width_custom_property(value))
}

pub(super) fn parse_border_width_declaration(
    value: &str,
) -> Option<[LocalCascadeDeclaration<BorderWidthDeclarationValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    if let Some(values) = parse_border_width_edges(value) {
        return Some(values.map(|value| {
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(value))
        }));
    }
    let (name_hash, fallback) = parse_border_width_shorthand_custom_property(value)?;
    Some(std::array::from_fn(|index| {
        LocalCascadeDeclaration::Value(match fallback {
            Some(values) => BorderWidthDeclarationValue::CustomPropertyShorthandFallback(
                name_hash,
                index as u8,
                values[index],
            ),
            None => BorderWidthDeclarationValue::CustomPropertyShorthand(name_hash, index as u8),
        })
    }))
}

pub(super) fn parse_local_physical_border_width_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>> {
    parse_local_cascade_declaration(value, parse_border_width_property)
}

pub(super) fn parse_border_width_value(value: &str) -> Option<NativeBorderWidthValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderWidthValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderWidthValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderWidthValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderWidthValue::Revert)
    } else {
        parse_dimension(value).map(NativeBorderWidthValue::Width)
    }
}

pub(super) fn parse_border_style_edges(value: &str) -> Option<[NativeBorderStyleValue; 4]> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_style_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    expand_box_edges(&values)
}

pub(super) fn parse_border_style_var_arguments(value: &str) -> Option<(u64, Option<&str>)> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    Some((parse_custom_property_name(name)?, fallback))
}

pub(super) fn parse_border_style_custom_property(
    value: &str,
) -> Option<BorderStyleDeclarationValue> {
    let (name_hash, fallback) = parse_border_style_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_border_style_value(fallback).map(|fallback| {
            BorderStyleDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(BorderStyleDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(super) fn parse_border_style_shorthand_custom_property(
    value: &str,
) -> Option<(u64, Option<[NativeBorderStyleValue; 4]>)> {
    let (name_hash, fallback) = parse_border_style_var_arguments(value)?;
    let fallback = match fallback {
        Some(fallback) => Some(parse_border_style_edges(fallback)?),
        None => None,
    };
    Some((name_hash, fallback))
}

pub(super) fn parse_border_style_property(value: &str) -> Option<BorderStyleDeclarationValue> {
    parse_border_style_value(value)
        .map(BorderStyleDeclarationValue::Value)
        .or_else(|| parse_border_style_custom_property(value))
}

pub(super) fn parse_border_style_declaration(
    value: &str,
) -> Option<[LocalCascadeDeclaration<BorderStyleDeclarationValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    if let Some(values) = parse_border_style_edges(value) {
        return Some(values.map(|value| {
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(value))
        }));
    }
    let (name_hash, fallback) = parse_border_style_shorthand_custom_property(value)?;
    Some(std::array::from_fn(|index| {
        LocalCascadeDeclaration::Value(match fallback {
            Some(values) => BorderStyleDeclarationValue::CustomPropertyShorthandFallback(
                name_hash,
                index as u8,
                values[index],
            ),
            None => BorderStyleDeclarationValue::CustomPropertyShorthand(name_hash, index as u8),
        })
    }))
}

pub(super) fn parse_local_physical_border_style_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>> {
    parse_local_cascade_declaration(value, parse_border_style_property)
}

pub(super) fn parse_border_color(value: &str) -> Option<[NativeBorderColorValue; 4]> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_color_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    match values.as_slice() {
        [all] => Some([*all; 4]),
        [top, right] => Some([*top, *right, *top, *right]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

pub(super) fn parse_border_color_shorthand_fallback(value: &str) -> Option<[NativeColor; 4]> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_color)
        .collect::<Option<Vec<_>>>()?;
    expand_box_edges(&values)
}

pub(super) fn parse_border_color_shorthand_custom_property(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 4]> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    let fallback = match fallback {
        Some(fallback) => Some(parse_border_color_shorthand_fallback(fallback)?),
        None => None,
    };
    Some(std::array::from_fn(|index| {
        LocalCascadeDeclaration::Value(match fallback {
            Some(values) => NativeBorderColorValue::CustomPropertyShorthandFallback(
                name_hash,
                index as u8,
                values[index],
            ),
            None => NativeBorderColorValue::CustomPropertyShorthand(name_hash, index as u8),
        })
    }))
}

pub(super) fn parse_border_color_declaration(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    parse_border_color_shorthand_custom_property(value).or_else(|| {
        parse_border_color(value).map(|values| values.map(LocalCascadeDeclaration::Value))
    })
}

pub(super) fn parse_border_color_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderColorValue>> {
    parse_local_cascade_declaration(value, parse_border_color_value)
}

pub(super) fn parse_logical_border_width_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<BorderWidthDeclarationValue>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    let values = split_css_value_tokens(value)?;
    let declarations = match values.as_slice() {
        [value] => [
            LocalCascadeDeclaration::Value(parse_logical_border_width_value(value, 0)?),
            LocalCascadeDeclaration::Value(parse_logical_border_width_value(value, 1)?),
        ],
        [start, end] => [
            LocalCascadeDeclaration::Value(parse_logical_border_width_value(start, 0)?),
            LocalCascadeDeclaration::Value(parse_logical_border_width_value(end, 1)?),
        ],
        _ => return None,
    };
    if declarations.iter().any(|declaration| {
        matches!(
            declaration,
            LocalCascadeDeclaration::Value(value) if value.is_css_wide()
        )
    }) && values.len() != 1
    {
        return None;
    }
    Some(declarations)
}

pub(super) fn parse_logical_border_width_value(
    value: &str,
    side: u8,
) -> Option<BorderWidthDeclarationValue> {
    if let Some(value) = parse_border_width_value(value) {
        return Some(BorderWidthDeclarationValue::Value(value));
    }
    let (name_hash, fallback) = parse_border_width_var_arguments(value)?;
    match fallback {
        Some(fallback) => Some(BorderWidthDeclarationValue::CustomPropertyPairFallback(
            name_hash,
            side,
            parse_logical_border_width_fallback(fallback, side)?,
        )),
        None => Some(BorderWidthDeclarationValue::CustomPropertyPair(
            name_hash, side,
        )),
    }
}

pub(super) fn parse_logical_border_width_fallback(
    value: &str,
    side: u8,
) -> Option<NativeBorderWidthValue> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_width_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    match values.as_slice() {
        [value] => Some(*value),
        [start, end] => Some(if side == 0 { *start } else { *end }),
        _ => None,
    }
}

pub(super) fn parse_logical_border_style_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<BorderStyleDeclarationValue>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    let values = split_css_value_tokens(value)?;
    let declarations = match values.as_slice() {
        [value] => [
            LocalCascadeDeclaration::Value(parse_logical_border_style_value(value, 0)?),
            LocalCascadeDeclaration::Value(parse_logical_border_style_value(value, 1)?),
        ],
        [start, end] => [
            LocalCascadeDeclaration::Value(parse_logical_border_style_value(start, 0)?),
            LocalCascadeDeclaration::Value(parse_logical_border_style_value(end, 1)?),
        ],
        _ => return None,
    };
    if declarations.iter().any(|declaration| {
        matches!(
            declaration,
            LocalCascadeDeclaration::Value(value) if value.is_css_wide()
        )
    }) && values.len() != 1
    {
        return None;
    }
    Some(declarations)
}

pub(super) fn parse_logical_border_style_value(
    value: &str,
    side: u8,
) -> Option<BorderStyleDeclarationValue> {
    if let Some(value) = parse_border_style_value(value) {
        return Some(BorderStyleDeclarationValue::Value(value));
    }
    let (name_hash, fallback) = parse_border_style_var_arguments(value)?;
    match fallback {
        Some(fallback) => Some(BorderStyleDeclarationValue::CustomPropertyPairFallback(
            name_hash,
            side,
            parse_logical_border_style_fallback(fallback, side)?,
        )),
        None => Some(BorderStyleDeclarationValue::CustomPropertyPair(
            name_hash, side,
        )),
    }
}

pub(super) fn parse_logical_border_style_fallback(
    value: &str,
    side: u8,
) -> Option<NativeBorderStyleValue> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_style_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    match values.as_slice() {
        [value] => Some(*value),
        [start, end] => Some(if side == 0 { *start } else { *end }),
        _ => None,
    }
}

pub(super) fn parse_logical_border_color_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    let values = split_css_value_tokens(value)?;
    let declarations = match values.as_slice() {
        [value] => [
            LocalCascadeDeclaration::Value(parse_logical_border_color_value(value, 0)?),
            LocalCascadeDeclaration::Value(parse_logical_border_color_value(value, 1)?),
        ],
        [start, end] => [
            LocalCascadeDeclaration::Value(parse_logical_border_color_value(start, 0)?),
            LocalCascadeDeclaration::Value(parse_logical_border_color_value(end, 1)?),
        ],
        _ => return None,
    };
    if values.len() != 1
        && declarations.iter().any(|declaration| {
            matches!(
                declaration,
                LocalCascadeDeclaration::Value(value) if value.is_css_wide()
            )
        })
    {
        return None;
    }
    Some(declarations)
}

pub(super) fn parse_logical_border_color_value(
    value: &str,
    side: u8,
) -> Option<NativeBorderColorValue> {
    if let Some(value) = parse_border_color_value(value) {
        return Some(match value {
            NativeBorderColorValue::CustomProperty(name_hash) => {
                NativeBorderColorValue::CustomPropertyPair(name_hash, side)
            }
            NativeBorderColorValue::CustomPropertyFallback(name_hash, fallback) => {
                NativeBorderColorValue::CustomPropertyPairFallback(name_hash, side, fallback)
            }
            value => value,
        });
    }
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_logical_border_color_fallback(fallback, side).map(|fallback| {
            NativeBorderColorValue::CustomPropertyPairFallback(name_hash, side, fallback)
        }),
        None => Some(NativeBorderColorValue::CustomPropertyPair(name_hash, side)),
    }
}

pub(super) fn parse_logical_border_color_fallback(value: &str, side: u8) -> Option<NativeColor> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_color)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [value] => Some(*value),
        [start, end] => Some(if side == 0 { *start } else { *end }),
        _ => None,
    }
}

pub(super) fn parse_border_color_value(value: &str) -> Option<NativeBorderColorValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("currentColor") {
        Some(NativeBorderColorValue::CurrentColor)
    } else if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderColorValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderColorValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderColorValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderColorValue::Revert)
    } else if let Some(value) = parse_color_custom_property(value) {
        match value {
            NativeColorValue::Color(color) => Some(NativeBorderColorValue::Color(color)),
            NativeColorValue::CurrentColor => Some(NativeBorderColorValue::CurrentColor),
            NativeColorValue::Inherit => Some(NativeBorderColorValue::Inherit),
            NativeColorValue::Unset => Some(NativeBorderColorValue::Unset),
            NativeColorValue::Initial => Some(NativeBorderColorValue::Initial),
            NativeColorValue::Revert => Some(NativeBorderColorValue::Revert),
            NativeColorValue::CustomProperty(name_hash) => {
                Some(NativeBorderColorValue::CustomProperty(name_hash))
            }
            NativeColorValue::CustomPropertyFallback(name_hash, fallback) => Some(
                NativeBorderColorValue::CustomPropertyFallback(name_hash, fallback),
            ),
        }
    } else {
        parse_color(value).map(NativeBorderColorValue::Color)
    }
}

impl NativeBorderColorValue {
    const fn is_css_wide(self) -> bool {
        matches!(
            self,
            Self::Inherit | Self::Unset | Self::Initial | Self::Revert
        )
    }
}
