use super::*;

pub(in crate::native_engine::css) fn parse_native_border_color_custom_property(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderColorValue>> {
    parse_border_color_value(value)
        .map(LocalCascadeDeclaration::Value)
        .or_else(|| parse_border_declaration(value).and_then(project_border_color_declaration))
}

pub(in crate::native_engine::css) fn parse_native_logical_border_color_custom_property(
    value: &str,
    side: u8,
) -> Option<LocalCascadeDeclaration<NativeBorderColorValue>> {
    parse_logical_border_color_value(value, side)
        .map(LocalCascadeDeclaration::Value)
        .or_else(|| {
            parse_logical_border_color_fallback(value, side).map(|fallback| {
                LocalCascadeDeclaration::Value(NativeBorderColorValue::Color(fallback))
            })
        })
        .or_else(|| parse_border_declaration(value).and_then(project_border_color_declaration))
}

pub(in crate::native_engine::css) fn resolve_native_border_color_shorthand_custom_property(
    name_hash: u64,
    side: u8,
    current_color: NativeColor,
    inherited_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<NativeColor>> {
    custom_properties
        .get(&name_hash)
        .and_then(|value| parse_border_color_declaration(value))
        .and_then(|declarations| declarations.get(side as usize).copied())
        .and_then(|declaration| match declaration {
            LocalCascadeDeclaration::Value(value) => resolve_native_border_color_value(
                value,
                current_color,
                inherited_color,
                custom_properties,
                depth.saturating_add(1),
            ),
            LocalCascadeDeclaration::RevertLayer => None,
            LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
        })
}

pub(in crate::native_engine::css) fn resolve_native_border_color_value(
    value: NativeBorderColorValue,
    current_color: NativeColor,
    inherited_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<NativeColor>> {
    match value {
        NativeBorderColorValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_native_border_color_custom_property(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_border_color_value(
                        value,
                        current_color,
                        inherited_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
        }
        NativeBorderColorValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_native_border_color_custom_property(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_border_color_value(
                        value,
                        current_color,
                        inherited_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
                .or(Some(Some(fallback)))
        }
        NativeBorderColorValue::CustomPropertyShorthand(name_hash, side)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            resolve_native_border_color_shorthand_custom_property(
                name_hash,
                side,
                current_color,
                inherited_color,
                custom_properties,
                depth,
            )
        }
        NativeBorderColorValue::CustomPropertyShorthandFallback(name_hash, side, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            resolve_native_border_color_shorthand_custom_property(
                name_hash,
                side,
                current_color,
                inherited_color,
                custom_properties,
                depth,
            )
            .or(Some(Some(fallback)))
        }
        NativeBorderColorValue::CustomPropertyPair(name_hash, side)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_native_logical_border_color_custom_property(value, side))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_border_color_value(
                        value,
                        current_color,
                        inherited_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
        }
        NativeBorderColorValue::CustomPropertyPairFallback(name_hash, side, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_native_logical_border_color_custom_property(value, side))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_border_color_value(
                        value,
                        current_color,
                        inherited_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
                .or(Some(Some(fallback)))
        }
        NativeBorderColorValue::CustomProperty(_)
        | NativeBorderColorValue::CustomPropertyFallback(_, _)
        | NativeBorderColorValue::CustomPropertyShorthand(_, _)
        | NativeBorderColorValue::CustomPropertyShorthandFallback(_, _, _)
        | NativeBorderColorValue::CustomPropertyPair(_, _)
        | NativeBorderColorValue::CustomPropertyPairFallback(_, _, _) => None,
        value => Some(Some(value.resolve(current_color, inherited_color))),
    }
}

pub(in crate::native_engine::css) fn resolve_local_border_color_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>>;
        MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS],
    current_color: NativeColor,
    inherited_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeColor> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => resolve_native_border_color_value(
            value,
            current_color,
            inherited_color,
            custom_properties,
            0,
        ),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

pub(in crate::native_engine::css) fn resolve_border_width_declaration(
    declaration: LocalCascadeDeclaration<BorderWidthDeclarationValue>,
    inherited_width: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<BorderWidthDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_border_width_value(value, inherited_width, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(BorderWidthDeclarationValue::Value(
            NativeBorderWidthValue::Width(inherited_width),
        )),
        LocalCascadeDeclaration::Reset => Some(BorderWidthDeclarationValue::Value(
            NativeBorderWidthValue::Width(0),
        )),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_border_width_value(
    value: BorderWidthDeclarationValue,
    inherited_width: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<BorderWidthDeclarationValue> {
    match value {
        BorderWidthDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    parse_border_declaration(value)
                        .and_then(project_border_width_declaration)
                        .or_else(|| parse_local_physical_border_width_side_declaration(value))
                })
                .and_then(|declaration| {
                    resolve_border_width_declaration(
                        declaration,
                        inherited_width,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BorderWidthDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    parse_border_declaration(value)
                        .and_then(project_border_width_declaration)
                        .or_else(|| parse_local_physical_border_width_side_declaration(value))
                })
                .and_then(|declaration| {
                    resolve_border_width_declaration(
                        declaration,
                        inherited_width,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| {
                    resolve_border_width_value(
                        BorderWidthDeclarationValue::Value(fallback),
                        inherited_width,
                        custom_properties,
                        depth,
                    )
                })
        }
        BorderWidthDeclarationValue::CustomPropertyShorthand(name_hash, edge)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_width_declaration(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_border_width_declaration(
                        declaration,
                        inherited_width,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BorderWidthDeclarationValue::CustomPropertyShorthandFallback(name_hash, edge, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_width_declaration(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_border_width_declaration(
                        declaration,
                        inherited_width,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| {
                    resolve_border_width_value(
                        BorderWidthDeclarationValue::Value(fallback),
                        inherited_width,
                        custom_properties,
                        depth,
                    )
                })
        }
        BorderWidthDeclarationValue::CustomPropertyPair(name_hash, side)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_logical_border_width_pair(value))
                .and_then(|declarations| declarations.get(side as usize).copied())
                .and_then(|declaration| {
                    resolve_border_width_declaration(
                        declaration,
                        inherited_width,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BorderWidthDeclarationValue::CustomPropertyPairFallback(name_hash, side, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_logical_border_width_pair(value))
                .and_then(|declarations| declarations.get(side as usize).copied())
                .and_then(|declaration| {
                    resolve_border_width_declaration(
                        declaration,
                        inherited_width,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| {
                    resolve_border_width_value(
                        BorderWidthDeclarationValue::Value(fallback),
                        inherited_width,
                        custom_properties,
                        depth,
                    )
                })
        }
        BorderWidthDeclarationValue::CustomProperty(_)
        | BorderWidthDeclarationValue::CustomPropertyFallback(_, _)
        | BorderWidthDeclarationValue::CustomPropertyShorthand(_, _)
        | BorderWidthDeclarationValue::CustomPropertyShorthandFallback(_, _, _)
        | BorderWidthDeclarationValue::CustomPropertyPair(_, _)
        | BorderWidthDeclarationValue::CustomPropertyPairFallback(_, _, _) => None,
        BorderWidthDeclarationValue::Value(value) => {
            Some(BorderWidthDeclarationValue::Value(match value {
                NativeBorderWidthValue::Width(width) => NativeBorderWidthValue::Width(width),
                NativeBorderWidthValue::Inherit => NativeBorderWidthValue::Width(inherited_width),
                NativeBorderWidthValue::Unset
                | NativeBorderWidthValue::Initial
                | NativeBorderWidthValue::Revert => NativeBorderWidthValue::Width(0),
            }))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_local_border_width_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<BorderWidthDeclarationValue>>>;
        MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS],
    inherited_width: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, None, |declaration| {
        resolve_border_width_declaration(declaration, inherited_width, custom_properties, 0).map(
            |value| match value {
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(width)) => {
                    Some(width)
                }
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Inherit)
                | BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Unset)
                | BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Initial)
                | BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Revert)
                | BorderWidthDeclarationValue::CustomProperty(_)
                | BorderWidthDeclarationValue::CustomPropertyFallback(_, _)
                | BorderWidthDeclarationValue::CustomPropertyShorthand(_, _)
                | BorderWidthDeclarationValue::CustomPropertyShorthandFallback(_, _, _)
                | BorderWidthDeclarationValue::CustomPropertyPair(_, _)
                | BorderWidthDeclarationValue::CustomPropertyPairFallback(_, _, _) => None,
            },
        )
    })
}

pub(in crate::native_engine::css) fn resolve_border_style_declaration(
    declaration: LocalCascadeDeclaration<BorderStyleDeclarationValue>,
    inherited_style: NativeBorderStyleValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<BorderStyleDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_border_style_value(value, inherited_style, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => {
            Some(BorderStyleDeclarationValue::Value(inherited_style))
        }
        LocalCascadeDeclaration::Reset => Some(BorderStyleDeclarationValue::Value(
            NativeBorderStyleValue::None,
        )),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_border_style_value(
    value: BorderStyleDeclarationValue,
    inherited_style: NativeBorderStyleValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<BorderStyleDeclarationValue> {
    match value {
        BorderStyleDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    parse_border_declaration(value)
                        .and_then(project_border_style_declaration)
                        .or_else(|| parse_local_physical_border_style_side_declaration(value))
                })
                .and_then(|declaration| {
                    resolve_border_style_declaration(
                        declaration,
                        inherited_style,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BorderStyleDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    parse_border_declaration(value)
                        .and_then(project_border_style_declaration)
                        .or_else(|| parse_local_physical_border_style_side_declaration(value))
                })
                .and_then(|declaration| {
                    resolve_border_style_declaration(
                        declaration,
                        inherited_style,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| {
                    resolve_border_style_value(
                        BorderStyleDeclarationValue::Value(fallback),
                        inherited_style,
                        custom_properties,
                        depth,
                    )
                })
        }
        BorderStyleDeclarationValue::CustomPropertyShorthand(name_hash, edge)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_style_declaration(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_border_style_declaration(
                        declaration,
                        inherited_style,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BorderStyleDeclarationValue::CustomPropertyShorthandFallback(name_hash, edge, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_style_declaration(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_border_style_declaration(
                        declaration,
                        inherited_style,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| {
                    resolve_border_style_value(
                        BorderStyleDeclarationValue::Value(fallback),
                        inherited_style,
                        custom_properties,
                        depth,
                    )
                })
        }
        BorderStyleDeclarationValue::CustomPropertyPair(name_hash, side)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_logical_border_style_pair(value))
                .and_then(|declarations| declarations.get(side as usize).copied())
                .and_then(|declaration| {
                    resolve_border_style_declaration(
                        declaration,
                        inherited_style,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BorderStyleDeclarationValue::CustomPropertyPairFallback(name_hash, side, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_logical_border_style_pair(value))
                .and_then(|declarations| declarations.get(side as usize).copied())
                .and_then(|declaration| {
                    resolve_border_style_declaration(
                        declaration,
                        inherited_style,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| {
                    resolve_border_style_value(
                        BorderStyleDeclarationValue::Value(fallback),
                        inherited_style,
                        custom_properties,
                        depth,
                    )
                })
        }
        BorderStyleDeclarationValue::CustomProperty(_)
        | BorderStyleDeclarationValue::CustomPropertyFallback(_, _)
        | BorderStyleDeclarationValue::CustomPropertyShorthand(_, _)
        | BorderStyleDeclarationValue::CustomPropertyShorthandFallback(_, _, _)
        | BorderStyleDeclarationValue::CustomPropertyPair(_, _)
        | BorderStyleDeclarationValue::CustomPropertyPairFallback(_, _, _) => None,
        BorderStyleDeclarationValue::Value(value) => {
            Some(BorderStyleDeclarationValue::Value(match value {
                NativeBorderStyleValue::Paint(style) => NativeBorderStyleValue::Paint(style),
                NativeBorderStyleValue::None => NativeBorderStyleValue::None,
                NativeBorderStyleValue::Hidden => NativeBorderStyleValue::Hidden,
                NativeBorderStyleValue::Inherit => inherited_style,
                NativeBorderStyleValue::Unset
                | NativeBorderStyleValue::Initial
                | NativeBorderStyleValue::Revert => NativeBorderStyleValue::None,
            }))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_local_border_style_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<BorderStyleDeclarationValue>>>;
        MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS],
    inherited_style: NativeBorderStyleValue,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeBorderStyleValue> {
    resolve_alignment_candidates(candidates, None, |declaration| {
        resolve_border_style_declaration(declaration, inherited_style, custom_properties, 0).map(
            |value| match value {
                BorderStyleDeclarationValue::Value(value) => Some(value),
                BorderStyleDeclarationValue::CustomProperty(_)
                | BorderStyleDeclarationValue::CustomPropertyFallback(_, _)
                | BorderStyleDeclarationValue::CustomPropertyShorthand(_, _)
                | BorderStyleDeclarationValue::CustomPropertyShorthandFallback(_, _, _)
                | BorderStyleDeclarationValue::CustomPropertyPair(_, _)
                | BorderStyleDeclarationValue::CustomPropertyPairFallback(_, _, _) => None,
            },
        )
    })
}

pub(in crate::native_engine::css) fn resolve_border_radius_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderRadiusValue>,
    index: usize,
    inherited_radius: NativeBorderRadius,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_border_radius_value(value, index, inherited_radius, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(inherited_radius.corner(index)),
        LocalCascadeDeclaration::Reset => Some(0),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_border_radius_value(
    value: NativeBorderRadiusValue,
    index: usize,
    inherited_radius: NativeBorderRadius,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match value {
        NativeBorderRadiusValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_radius_declaration(value))
                .and_then(|declaration| {
                    resolve_border_radius_declaration(
                        declaration,
                        index,
                        inherited_radius,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBorderRadiusValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_radius_declaration(value))
                .and_then(|declaration| {
                    resolve_border_radius_declaration(
                        declaration,
                        index,
                        inherited_radius,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| Some(fallback.corner(index)))
        }
        NativeBorderRadiusValue::CustomPropertyCorner(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_radius_corner_declaration(value))
                .and_then(|declaration| {
                    resolve_border_radius_declaration(
                        declaration,
                        index,
                        inherited_radius,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBorderRadiusValue::CustomPropertyCornerFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_border_radius_corner_declaration(value))
                .and_then(|declaration| {
                    resolve_border_radius_declaration(
                        declaration,
                        index,
                        inherited_radius,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeBorderRadiusValue::Radius(radius) => Some(radius.corner(index)),
        NativeBorderRadiusValue::Corner(radius) => Some(radius),
        NativeBorderRadiusValue::Inherit => Some(inherited_radius.corner(index)),
        NativeBorderRadiusValue::Unset
        | NativeBorderRadiusValue::Initial
        | NativeBorderRadiusValue::Revert => Some(0),
        NativeBorderRadiusValue::CustomProperty(_)
        | NativeBorderRadiusValue::CustomPropertyFallback(_, _)
        | NativeBorderRadiusValue::CustomPropertyCorner(_)
        | NativeBorderRadiusValue::CustomPropertyCornerFallback(_, _) => None,
    }
}

pub(in crate::native_engine::css) fn resolve_local_border_radius_declaration(
    candidates: [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
        MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4],
    inherited_radius: NativeBorderRadius,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeBorderRadius {
    let [top_left, top_right, bottom_right, bottom_left] = std::array::from_fn(|index| {
        resolve_alignment_candidates(candidates[index], None, |declaration| {
            resolve_border_radius_declaration(
                declaration,
                index,
                inherited_radius,
                custom_properties,
                0,
            )
            .map(Some)
        })
        .unwrap_or_default()
    });
    NativeBorderRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

pub(in crate::native_engine::css) fn resolve_alignment_candidates<
    T: Copy,
    U: Copy,
    const N: usize,
>(
    candidates: [Option<CascadeValue<T>>; N],
    inherited: U,
    value: impl Fn(T) -> Option<U>,
) -> U {
    let mut blocked = [false; N];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if value(candidate.value).is_none() {
            blocked[layer] = true;
            continue;
        }
        return value(candidate.value).unwrap_or(inherited);
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_decoration_skip_spaces_value(
    value: NativeTextDecorationSkipSpacesDeclaration,
    inherited: NativeTextDecorationSkipSpaces,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeTextDecorationSkipSpaces> {
    match value {
        NativeTextDecorationSkipSpacesDeclaration::Value(value) => Some(value),
        NativeTextDecorationSkipSpacesDeclaration::Inherit
        | NativeTextDecorationSkipSpacesDeclaration::Unset
        | NativeTextDecorationSkipSpacesDeclaration::Revert => Some(inherited),
        NativeTextDecorationSkipSpacesDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_skip_spaces(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_skip_spaces_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeTextDecorationSkipSpacesDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_skip_spaces(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_skip_spaces_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeTextDecorationSkipSpacesDeclaration::CustomProperty(_)
        | NativeTextDecorationSkipSpacesDeclaration::CustomPropertyFallback(_, _)
        | NativeTextDecorationSkipSpacesDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_decoration_skip_spaces(
    candidates: [Option<CascadeValue<NativeTextDecorationSkipSpacesDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: NativeTextDecorationSkipSpaces,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeTextDecorationSkipSpaces {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationSkipSpacesDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        let Some(value) = resolve_native_text_decoration_skip_spaces_value(
            candidate.value,
            inherited,
            custom_properties,
            0,
        ) else {
            blocked[layer] = true;
            continue;
        };
        return value;
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_decoration_style_value(
    value: NativeTextDecorationStyleDeclaration,
    inherited: NativeTextDecorationStyle,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeTextDecorationStyle> {
    match value {
        NativeTextDecorationStyleDeclaration::Value(value) => Some(value),
        NativeTextDecorationStyleDeclaration::Inherit
        | NativeTextDecorationStyleDeclaration::Unset
        | NativeTextDecorationStyleDeclaration::Revert => Some(inherited),
        NativeTextDecorationStyleDeclaration::Initial => Some(NativeTextDecorationStyle::Solid),
        NativeTextDecorationStyleDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_style(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_style_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeTextDecorationStyleDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_style(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_style_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeTextDecorationStyleDeclaration::CustomProperty(_)
        | NativeTextDecorationStyleDeclaration::CustomPropertyFallback(_, _)
        | NativeTextDecorationStyleDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_decoration_style(
    candidates: [Option<CascadeValue<NativeTextDecorationStyleDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: NativeTextDecorationStyle,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeTextDecorationStyle {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationStyleDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        let Some(value) = resolve_native_text_decoration_style_value(
            candidate.value,
            inherited,
            custom_properties,
            0,
        ) else {
            blocked[layer] = true;
            continue;
        };
        return value;
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_decoration_thickness_value(
    value: NativeTextDecorationThicknessDeclaration,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match value {
        NativeTextDecorationThicknessDeclaration::Value(value) => Some(value),
        NativeTextDecorationThicknessDeclaration::Inherit
        | NativeTextDecorationThicknessDeclaration::Unset
        | NativeTextDecorationThicknessDeclaration::Revert => Some(inherited),
        NativeTextDecorationThicknessDeclaration::Initial => Some(1),
        NativeTextDecorationThicknessDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_thickness(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_thickness_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeTextDecorationThicknessDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_thickness(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_thickness_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeTextDecorationThicknessDeclaration::CustomProperty(_)
        | NativeTextDecorationThicknessDeclaration::CustomPropertyFallback(_, _)
        | NativeTextDecorationThicknessDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_decoration_thickness(
    candidates: [Option<CascadeValue<NativeTextDecorationThicknessDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationThicknessDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        let Some(value) = resolve_native_text_decoration_thickness_value(
            candidate.value,
            inherited,
            custom_properties,
            0,
        ) else {
            blocked[layer] = true;
            continue;
        };
        return value;
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_underline_offset_value(
    value: NativeTextUnderlineOffsetDeclaration,
    inherited: i32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<i32> {
    match value {
        NativeTextUnderlineOffsetDeclaration::Value(value) => Some(value),
        NativeTextUnderlineOffsetDeclaration::Inherit
        | NativeTextUnderlineOffsetDeclaration::Unset
        | NativeTextUnderlineOffsetDeclaration::Revert => Some(inherited),
        NativeTextUnderlineOffsetDeclaration::Initial => Some(0),
        NativeTextUnderlineOffsetDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_underline_offset(value))
                .and_then(|declaration| {
                    resolve_native_text_underline_offset_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeTextUnderlineOffsetDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_underline_offset(value))
                .and_then(|declaration| {
                    resolve_native_text_underline_offset_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeTextUnderlineOffsetDeclaration::CustomProperty(_)
        | NativeTextUnderlineOffsetDeclaration::CustomPropertyFallback(_, _)
        | NativeTextUnderlineOffsetDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_underline_offset(
    candidates: [Option<CascadeValue<NativeTextUnderlineOffsetDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: i32,
    custom_properties: &BTreeMap<u64, String>,
) -> i32 {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextUnderlineOffsetDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        let Some(value) = resolve_native_text_underline_offset_value(
            candidate.value,
            inherited,
            custom_properties,
            0,
        ) else {
            blocked[layer] = true;
            continue;
        };
        return value;
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_decoration_value(
    value: NativeTextDecorationDeclaration,
    inherited: TextDecorationValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextDecorationValue> {
    match value {
        NativeTextDecorationDeclaration::Value(value) => Some(value),
        NativeTextDecorationDeclaration::Inherit
        | NativeTextDecorationDeclaration::Unset
        | NativeTextDecorationDeclaration::Revert => Some(inherited),
        NativeTextDecorationDeclaration::Initial => Some(TextDecorationValue::none()),
        NativeTextDecorationDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_declaration(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeTextDecorationDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_declaration(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeTextDecorationDeclaration::CustomProperty(_)
        | NativeTextDecorationDeclaration::CustomPropertyFallback(_, _)
        | NativeTextDecorationDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_decoration(
    candidates: [Option<CascadeValue<NativeTextDecorationDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: TextDecorationValue,
    custom_properties: &BTreeMap<u64, String>,
) -> TextDecorationValue {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        let Some(value) =
            resolve_native_text_decoration_value(candidate.value, inherited, custom_properties, 0)
        else {
            blocked[layer] = true;
            continue;
        };
        return value;
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_decoration_color_value(
    value: NativeTextDecorationColorDeclaration,
    current_color: NativeColor,
    inherited_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeColor> {
    match value {
        NativeTextDecorationColorDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_color(value))
                .and_then(|value| match value {
                    NativeTextDecorationColorDeclaration::RevertLayer => None,
                    value => resolve_native_text_decoration_color_value(
                        value,
                        current_color,
                        inherited_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                })
        }
        NativeTextDecorationColorDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_color(value))
                .and_then(|value| match value {
                    NativeTextDecorationColorDeclaration::RevertLayer => None,
                    value => resolve_native_text_decoration_color_value(
                        value,
                        current_color,
                        inherited_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                })
                .or(Some(fallback))
        }
        NativeTextDecorationColorDeclaration::CustomProperty(_)
        | NativeTextDecorationColorDeclaration::CustomPropertyFallback(_, _)
        | NativeTextDecorationColorDeclaration::RevertLayer => None,
        NativeTextDecorationColorDeclaration::Value(value) => Some(value),
        NativeTextDecorationColorDeclaration::CurrentColor => Some(current_color),
        NativeTextDecorationColorDeclaration::Inherit => Some(inherited_color),
        NativeTextDecorationColorDeclaration::Unset
        | NativeTextDecorationColorDeclaration::Initial
        | NativeTextDecorationColorDeclaration::Revert => Some(current_color),
    }
}

pub(in crate::native_engine::css) fn resolve_text_decoration_color(
    candidates: [Option<CascadeValue<NativeTextDecorationColorDeclaration>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    current_color: NativeColor,
    inherited_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeColor> {
    let mut blocked = [false; MAX_NATIVE_PAINT_CASCADE_LAYERS];
    loop {
        let (layer, candidate) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })?;
        if matches!(
            candidate.value,
            NativeTextDecorationColorDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return resolve_native_text_decoration_color_value(
            candidate.value,
            current_color,
            inherited_color,
            custom_properties,
            0,
        );
    }
}

pub(in crate::native_engine::css) fn resolve_native_text_decoration_skip_ink_value(
    value: NativeTextDecorationSkipInkDeclaration,
    inherited: NativeTextDecorationSkipInk,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeTextDecorationSkipInk> {
    match value {
        NativeTextDecorationSkipInkDeclaration::Value(value) => Some(value),
        NativeTextDecorationSkipInkDeclaration::Inherit
        | NativeTextDecorationSkipInkDeclaration::Unset
        | NativeTextDecorationSkipInkDeclaration::Revert => Some(inherited),
        NativeTextDecorationSkipInkDeclaration::Initial => Some(NativeTextDecorationSkipInk::Auto),
        NativeTextDecorationSkipInkDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_skip_ink(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_skip_ink_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeTextDecorationSkipInkDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_decoration_skip_ink(value))
                .and_then(|declaration| {
                    resolve_native_text_decoration_skip_ink_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeTextDecorationSkipInkDeclaration::CustomProperty(_)
        | NativeTextDecorationSkipInkDeclaration::CustomPropertyFallback(_, _)
        | NativeTextDecorationSkipInkDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_decoration_skip_ink(
    candidates: [Option<CascadeValue<NativeTextDecorationSkipInkDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: NativeTextDecorationSkipInk,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeTextDecorationSkipInk {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationSkipInkDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        let Some(value) = resolve_native_text_decoration_skip_ink_value(
            candidate.value,
            inherited,
            custom_properties,
            0,
        ) else {
            blocked[layer] = true;
            continue;
        };
        return value;
    }
}

pub(in crate::native_engine::css) fn gap_precedes<T, U>(
    candidate: &GapCascadeValue<T>,
    current: &GapCascadeValue<U>,
) -> bool {
    (
        candidate.inline,
        candidate.specificity,
        candidate.order,
        candidate.declaration_order,
    ) > (
        current.inline,
        current.specificity,
        current.order,
        current.declaration_order,
    )
}

pub(in crate::native_engine::css) fn gap_wins<T>(
    specificity: u16,
    order: usize,
    declaration_order: usize,
    inline: bool,
    current: Option<GapCascadeValue<T>>,
) -> bool {
    current.is_none_or(|current| {
        (inline, specificity, order, declaration_order)
            > (
                current.inline,
                current.specificity,
                current.order,
                current.declaration_order,
            )
    })
}

pub(in crate::native_engine::css) fn set_gap_candidate<T: Copy>(
    slot: &mut Option<GapCascadeValue<T>>,
    value: T,
    specificity: u16,
    order: usize,
    declaration_order: usize,
    inline: bool,
) {
    if gap_wins(specificity, order, declaration_order, inline, *slot) {
        *slot = Some(GapCascadeValue {
            value,
            specificity,
            order,
            declaration_order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn select_gap_candidate(
    shorthand: Option<GapCascadeValue<GapComponentDeclaration>>,
    longhand: Option<GapCascadeValue<GapComponentDeclaration>>,
) -> Option<GapCascadeValue<GapComponentDeclaration>> {
    match (shorthand, longhand) {
        (Some(shorthand), Some(longhand)) if gap_precedes(&longhand, &shorthand) => Some(longhand),
        (Some(shorthand), _) => Some(shorthand),
        (None, Some(longhand)) => Some(longhand),
        (None, None) => None,
    }
}

pub(in crate::native_engine::css) fn resolve_gap_component_declaration(
    declaration: GapComponentDeclaration,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match declaration {
        GapComponentDeclaration::Value(value) => Some(value),
        GapComponentDeclaration::Inherit => Some(inherited),
        GapComponentDeclaration::Initial
        | GapComponentDeclaration::Unset
        | GapComponentDeclaration::Revert => Some(0),
        GapComponentDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_gap_component_declaration(value))
                .and_then(|declaration| {
                    resolve_gap_component_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        GapComponentDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_gap_component_declaration(value))
                .and_then(|declaration| {
                    resolve_gap_component_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        GapComponentDeclaration::CustomPropertyShorthand(name_hash, axis)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_gap_declaration(value))
                .map(|declaration| project_gap_shorthand_declaration(declaration, axis))
                .and_then(|declaration| {
                    resolve_gap_component_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        GapComponentDeclaration::CustomPropertyShorthandFallback(name_hash, axis, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_gap_declaration(value))
                .map(|declaration| project_gap_shorthand_declaration(declaration, axis))
                .and_then(|declaration| {
                    resolve_gap_component_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        GapComponentDeclaration::CustomProperty(_)
        | GapComponentDeclaration::CustomPropertyFallback(_, _)
        | GapComponentDeclaration::CustomPropertyShorthand(_, _)
        | GapComponentDeclaration::CustomPropertyShorthandFallback(_, _, _)
        | GapComponentDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_gap_axis(
    shorthand: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    longhand: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    let mut blocked = [false; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            (0..MAX_NATIVE_LOCAL_CASCADE_LAYERS)
                .rev()
                .find_map(|layer| {
                    if blocked[layer] {
                        None
                    } else {
                        select_gap_candidate(shorthand[layer], longhand[layer])
                            .map(|candidate| (layer, candidate))
                    }
                })
        else {
            return 0;
        };
        if let Some(value) =
            resolve_gap_component_declaration(candidate.value, inherited, custom_properties, 0)
        {
            return value;
        }
        blocked[layer] = true;
    }
}

pub(in crate::native_engine::css) fn apply_gap_declarations(
    declarations: NativeDeclarations,
    specificity: u16,
    order: usize,
    inline: bool,
    cascade: &mut GapCascade,
) {
    if let Some(value) = declarations.gap {
        let layer = local_cascade_layer(specificity, declarations.gap_important);
        let row = project_gap_shorthand_declaration(value, 0);
        let column = project_gap_shorthand_declaration(value, 1);
        set_gap_candidate(
            &mut cascade.shorthand_row[layer],
            row,
            specificity,
            order,
            declarations.gap_order,
            inline,
        );
        set_gap_candidate(
            &mut cascade.shorthand_column[layer],
            column,
            specificity,
            order,
            declarations.gap_order,
            inline,
        );
    }
    if let Some(value) = declarations.row_gap {
        let layer = local_cascade_layer(specificity, declarations.row_gap_important);
        set_gap_candidate(
            &mut cascade.row_gap[layer],
            value,
            specificity,
            order,
            declarations.row_gap_order,
            inline,
        );
    }
    if let Some(value) = declarations.column_gap {
        let layer = local_cascade_layer(specificity, declarations.column_gap_important);
        set_gap_candidate(
            &mut cascade.column_gap[layer],
            value,
            specificity,
            order,
            declarations.column_gap_order,
            inline,
        );
    }
}

pub(in crate::native_engine::css) fn text_cascade_layer(
    specificity: u16,
    important: bool,
) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_TEXT_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

pub(in crate::native_engine::css) fn apply_text_cascade_declaration<T: Copy>(
    declaration: Option<T>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<T>>; MAX_NATIVE_TEXT_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = text_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn apply_local_cascade_declaration<T: Copy>(
    declaration: Option<LocalCascadeDeclaration<T>>,
    specificity: u16,
    order: usize,
    inline: bool,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = cascade_layer_index(specificity);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn local_cascade_layer(
    specificity: u16,
    important: bool,
) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_LOCAL_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

pub(in crate::native_engine::css) fn apply_local_important_cascade_declaration<T: Copy>(
    declaration: Option<LocalCascadeDeclaration<T>>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<T>>>;
             MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = local_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn apply_flex_cascade_declaration<T: Copy>(
    declaration: Option<T>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<T>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = local_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn border_cascade_order(
    rule_order: usize,
    declaration_order: usize,
    inline: bool,
) -> usize {
    if inline {
        declaration_order
    } else {
        rule_order
            .saturating_mul(CASCADE_DECLARATION_ORDER_STRIDE)
            .saturating_add(declaration_order)
    }
}

pub(in crate::native_engine::css) fn radius_cascade_layer(
    specificity: u16,
    important: bool,
) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_RADIUS_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

pub(in crate::native_engine::css) fn apply_radius_cascade_declaration(
    declaration: Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
             MAX_NATIVE_RADIUS_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = radius_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn paint_cascade_layer(
    specificity: u16,
    important: bool,
) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_PAINT_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

pub(in crate::native_engine::css) fn apply_paint_cascade_declaration<T: Copy, const N: usize>(
    declaration: Option<T>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<T>>; N],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = paint_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

pub(in crate::native_engine::css) fn project_border_width_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CustomProperty(name_hash)) => Some(
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::CustomProperty(name_hash)),
        ),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CustomPropertyFallback(
            name_hash,
            fallback,
        )) => Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::CustomPropertyFallback(
                name_hash,
                NativeBorderWidthValue::Width(fallback.width()),
            ),
        )),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => {
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(border.width())),
            ))
        }
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteCurrentColor { width, .. }
            | NativeBorderDeclaration::CompleteNoneCurrentColor { width }
            | NativeBorderDeclaration::CompleteHiddenCurrentColor { width }
            | NativeBorderDeclaration::CompleteHidden { width, .. }
            | NativeBorderDeclaration::CompleteNone { width, .. },
        ) => Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(width)),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None | NativeBorderDeclaration::Hidden,
        ) => None,
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Inherit) => {
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Inherit),
            ))
        }
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Unset
            | NativeBorderDeclaration::Initial
            | NativeBorderDeclaration::Revert,
        ) => Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(0)),
        )),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(LocalCascadeDeclaration::Reset),
        LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
    }
}
pub(in crate::native_engine::css) fn project_logical_border_width_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>> {
    project_border_width_declaration(declaration)
}

pub(in crate::native_engine::css) fn project_border_style_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CustomProperty(name_hash)) => Some(
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomProperty(name_hash)),
        ),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CustomPropertyFallback(
            name_hash,
            fallback,
        )) => Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::CustomPropertyFallback(
                name_hash,
                NativeBorderStyleValue::Paint(fallback.style()),
            ),
        )),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => {
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(border.style())),
            ))
        }
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CompleteCurrentColor {
            style,
            ..
        }) => Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(style)),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteHidden { .. }
            | NativeBorderDeclaration::CompleteHiddenCurrentColor { .. }
            | NativeBorderDeclaration::Hidden,
        ) => Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Hidden),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteNone { .. }
            | NativeBorderDeclaration::CompleteNoneCurrentColor { .. }
            | NativeBorderDeclaration::None,
        ) => Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::None),
        )),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Inherit) => {
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Inherit),
            ))
        }
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Unset
            | NativeBorderDeclaration::Initial
            | NativeBorderDeclaration::Revert,
        ) => Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::None),
        )),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(LocalCascadeDeclaration::Reset),
        LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
    }
}

pub(in crate::native_engine::css) fn project_logical_border_style_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>> {
    project_border_style_declaration(declaration)
}

pub(in crate::native_engine::css) fn project_border_color_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<NativeBorderColorValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CustomProperty(name_hash)) => Some(
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomProperty(name_hash)),
        ),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CustomPropertyFallback(
            name_hash,
            fallback,
        )) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CustomPropertyFallback(name_hash, fallback.color()),
        )),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => Some(
            LocalCascadeDeclaration::Value(NativeBorderColorValue::Color(border.color())),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteCurrentColor { .. }
            | NativeBorderDeclaration::CompleteHiddenCurrentColor { .. }
            | NativeBorderDeclaration::CompleteNoneCurrentColor { .. },
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CurrentColor,
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteHidden { color, .. }
            | NativeBorderDeclaration::CompleteNone { color, .. },
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::Color(color),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None | NativeBorderDeclaration::Hidden,
        ) => None,
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Inherit) => Some(
            LocalCascadeDeclaration::Value(NativeBorderColorValue::Inherit),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Unset
            | NativeBorderDeclaration::Initial
            | NativeBorderDeclaration::Revert,
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CurrentColor,
        )),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(LocalCascadeDeclaration::Reset),
        LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
    }
}

pub(in crate::native_engine::css) fn apply_border_width_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativePhysicalBorderWidthCandidates,
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_width = declarations.border[index].and_then(project_border_width_declaration);
        apply_paint_cascade_declaration(
            border_width,
            specificity,
            border_cascade_order(rule_order, declarations.border_order[index], inline),
            inline,
            declarations.border_important[index],
            candidate,
        );
        apply_paint_cascade_declaration(
            declarations.border_width[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_width_order[index], inline),
            inline,
            declarations.border_width_important[index],
            candidate,
        );
    }
}

pub(in crate::native_engine::css) fn apply_border_style_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativePhysicalBorderStyleCandidates,
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_style = declarations.border[index].and_then(project_border_style_declaration);
        apply_paint_cascade_declaration(
            border_style,
            specificity,
            border_cascade_order(rule_order, declarations.border_order[index], inline),
            inline,
            declarations.border_important[index],
            candidate,
        );
        apply_paint_cascade_declaration(
            declarations.border_style[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_style_order[index], inline),
            inline,
            declarations.border_style_important[index],
            candidate,
        );
    }
}

pub(in crate::native_engine::css) fn apply_border_color_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>>;
             MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; 4],
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_order =
            border_cascade_order(rule_order, declarations.border_order[index], inline);
        let border_color = declarations.border[index].and_then(project_border_color_declaration);
        apply_paint_cascade_declaration(
            border_color,
            specificity,
            border_order,
            inline,
            declarations.border_important[index],
            candidate,
        );
        apply_paint_cascade_declaration(
            declarations.border_color[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_color_order[index], inline),
            inline,
            declarations.border_color_important[index],
            candidate,
        );
    }
}

pub(in crate::native_engine::css) fn apply_border_radius_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
             MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4],
) {
    if let Some(border_radius) = declarations.border_radius {
        let order = border_cascade_order(rule_order, declarations.border_radius_order, inline);
        for candidate in candidates.iter_mut() {
            apply_radius_cascade_declaration(
                Some(border_radius),
                specificity,
                order,
                inline,
                declarations.border_radius_important,
                candidate,
            );
        }
    }
    for (index, candidate) in candidates.iter_mut().enumerate() {
        apply_radius_cascade_declaration(
            declarations.border_radius_corners[index],
            specificity,
            border_cascade_order(
                rule_order,
                declarations.border_radius_corner_orders[index],
                inline,
            ),
            inline,
            declarations.border_radius_corner_important[index],
            candidate,
        );
    }
}

pub(in crate::native_engine::css) fn apply_logical_border_radius_cascade(
    declarations: &NativeLogicalBorderRadiusDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativeLogicalBorderRadiusCandidates,
) {
    for (index, declaration) in declarations.corners.iter().copied().enumerate() {
        apply_radius_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declarations.corner_orders[index], inline),
            inline,
            declarations.important[index],
            &mut candidates[index],
        );
    }
}
