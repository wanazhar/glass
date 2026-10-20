use super::*;

pub(in crate::native_engine::css) fn resolve_native_background_shorthand_source(
    source: &str,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeBackgroundShorthand> {
    if depth >= MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH {
        return None;
    }
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(source) {
        return custom_properties
            .get(&name_hash)
            .and_then(|value| {
                resolve_native_background_shorthand_source(
                    value,
                    custom_properties,
                    depth.saturating_add(1),
                )
            })
            .or_else(|| fallback.and_then(parse_background_shorthand));
    }
    parse_background_shorthand(source)
}

pub(in crate::native_engine::css) fn resolve_native_background_color_value(
    value: NativeBackgroundColorValue,
    inherited_background: Option<NativeColor>,
    current_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<NativeColor>> {
    match value {
        NativeBackgroundColorValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_color_declaration(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_background_color_value(
                        value,
                        inherited_background,
                        current_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
        }
        NativeBackgroundColorValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_color_declaration(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_background_color_value(
                        value,
                        inherited_background,
                        current_color,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
                .or(Some(Some(fallback)))
        }
        NativeBackgroundColorValue::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_color_value(
                        value.color,
                        inherited_background,
                        current_color,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundColorValue::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_color_value(
                        value.color,
                        inherited_background,
                        current_color,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(Some(fallback.resolve(current_color))))
        }
        NativeBackgroundColorValue::CustomProperty(_)
        | NativeBackgroundColorValue::CustomPropertyFallback(_, _) => None,
        value => Some(value.resolve(inherited_background, current_color)),
    }
}

pub(in crate::native_engine::css) fn resolve_local_background_color_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBackgroundColorValue>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    inherited_background: Option<NativeColor>,
    current_color: NativeColor,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeColor> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => resolve_native_background_color_value(
            value,
            inherited_background,
            current_color,
            custom_properties,
            0,
        ),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

pub(in crate::native_engine::css) fn resolve_native_background_image_value(
    value: NativeBackgroundImageValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<u32>> {
    match value {
        NativeBackgroundImageValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_image_value(value))
                .and_then(|value| {
                    resolve_native_background_image_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundImageValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_image_value(value))
                .and_then(|value| {
                    resolve_native_background_image_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeBackgroundImageValue::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_image_value(
                        value.image,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundImageValue::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_image_value(
                        value.image,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeBackgroundImageValue::CustomProperty(_)
        | NativeBackgroundImageValue::CustomPropertyFallback(_, _) => None,
        value => Some(value.resolve()),
    }
}

pub(in crate::native_engine::css) fn resolve_local_background_image_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBackgroundImageValue>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    custom_properties: &BTreeMap<u64, String>,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_native_background_image_value(value, custom_properties, 0)
        }
        LocalCascadeDeclaration::Inherit
        | LocalCascadeDeclaration::Reset
        | LocalCascadeDeclaration::RevertLayer => None,
    })
}
pub(in crate::native_engine::css) fn resolve_native_background_repeat_value(
    value: NativeBackgroundRepeat,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeBackgroundRepeat> {
    match value {
        NativeBackgroundRepeat::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_repeat_value(value))
                .and_then(|value| {
                    resolve_native_background_repeat_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundRepeat::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_repeat_value(value))
                .and_then(|value| {
                    resolve_native_background_repeat_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or({
                    Some(match fallback {
                        NativeBackgroundRepeatFallback::Repeat => NativeBackgroundRepeat::Repeat,
                        NativeBackgroundRepeatFallback::RepeatX => NativeBackgroundRepeat::RepeatX,
                        NativeBackgroundRepeatFallback::RepeatY => NativeBackgroundRepeat::RepeatY,
                        NativeBackgroundRepeatFallback::NoRepeat => {
                            NativeBackgroundRepeat::NoRepeat
                        }
                    })
                })
        }
        NativeBackgroundRepeat::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_repeat_value(
                        value.repeat,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundRepeat::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_repeat_value(
                        value.repeat,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or({
                    Some(match fallback {
                        NativeBackgroundRepeatFallback::Repeat => NativeBackgroundRepeat::Repeat,
                        NativeBackgroundRepeatFallback::RepeatX => NativeBackgroundRepeat::RepeatX,
                        NativeBackgroundRepeatFallback::RepeatY => NativeBackgroundRepeat::RepeatY,
                        NativeBackgroundRepeatFallback::NoRepeat => {
                            NativeBackgroundRepeat::NoRepeat
                        }
                    })
                })
        }
        NativeBackgroundRepeat::CustomProperty(_)
        | NativeBackgroundRepeat::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(in crate::native_engine::css) fn resolve_local_background_repeat_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBackgroundRepeat>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    custom_properties: &BTreeMap<u64, String>,
) -> NativeBackgroundRepeat {
    resolve_alignment_candidates(
        candidates,
        NativeBackgroundRepeat::default(),
        |declaration| match declaration {
            LocalCascadeDeclaration::Value(value) => {
                resolve_native_background_repeat_value(value, custom_properties, 0)
            }
            LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => {
                Some(NativeBackgroundRepeat::default())
            }
            LocalCascadeDeclaration::RevertLayer => None,
        },
    )
}

pub(in crate::native_engine::css) fn resolve_native_background_position_value(
    value: NativeBackgroundPositionDeclaration,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeBackgroundPosition> {
    match value {
        NativeBackgroundPositionDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_position_value(value))
                .and_then(|value| {
                    resolve_native_background_position_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundPositionDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_position_value(value))
                .and_then(|value| {
                    resolve_native_background_position_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeBackgroundPositionDeclaration::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_position_value(
                        NativeBackgroundPositionDeclaration::Value(value.position),
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundPositionDeclaration::CustomPropertyShorthandFallback(
            name_hash,
            fallback,
        ) if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH => custom_properties
            .get(&name_hash)
            .and_then(|value| {
                resolve_native_background_shorthand_source(
                    value,
                    custom_properties,
                    depth.saturating_add(1),
                )
            })
            .and_then(|value| {
                resolve_native_background_position_value(
                    NativeBackgroundPositionDeclaration::Value(value.position),
                    custom_properties,
                    depth.saturating_add(1),
                )
            })
            .or(Some(fallback)),
        NativeBackgroundPositionDeclaration::CustomProperty(_)
        | NativeBackgroundPositionDeclaration::CustomPropertyFallback(_, _)
        | NativeBackgroundPositionDeclaration::CustomPropertyShorthand(_)
        | NativeBackgroundPositionDeclaration::CustomPropertyShorthandFallback(_, _) => None,
        NativeBackgroundPositionDeclaration::Value(value) => Some(value),
    }
}

pub(in crate::native_engine::css) fn resolve_local_background_position_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBackgroundPositionDeclaration>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    custom_properties: &BTreeMap<u64, String>,
) -> NativeBackgroundPosition {
    resolve_alignment_candidates(
        candidates,
        NativeBackgroundPosition::default(),
        |declaration| match declaration {
            LocalCascadeDeclaration::Value(value) => {
                resolve_native_background_position_value(value, custom_properties, 0)
            }
            LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => {
                Some(NativeBackgroundPosition::default())
            }
            LocalCascadeDeclaration::RevertLayer => None,
        },
    )
}

pub(in crate::native_engine::css) fn resolve_native_background_size_value(
    value: NativeBackgroundSizeDeclaration,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeBackgroundSize> {
    match value {
        NativeBackgroundSizeDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_size_value(value))
                .and_then(|value| {
                    resolve_native_background_size_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundSizeDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_background_size_value(value))
                .and_then(|value| {
                    resolve_native_background_size_value(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeBackgroundSizeDeclaration::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_size_value(
                        NativeBackgroundSizeDeclaration::Value(value.size),
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        NativeBackgroundSizeDeclaration::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_native_background_shorthand_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .and_then(|value| {
                    resolve_native_background_size_value(
                        NativeBackgroundSizeDeclaration::Value(value.size),
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        NativeBackgroundSizeDeclaration::CustomProperty(_)
        | NativeBackgroundSizeDeclaration::CustomPropertyFallback(_, _)
        | NativeBackgroundSizeDeclaration::CustomPropertyShorthand(_)
        | NativeBackgroundSizeDeclaration::CustomPropertyShorthandFallback(_, _) => None,
        NativeBackgroundSizeDeclaration::Value(value) => Some(value),
    }
}

pub(in crate::native_engine::css) fn resolve_local_background_size_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBackgroundSizeDeclaration>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    custom_properties: &BTreeMap<u64, String>,
) -> NativeBackgroundSize {
    resolve_alignment_candidates(candidates, NativeBackgroundSize::default(), |declaration| {
        match declaration {
            LocalCascadeDeclaration::Value(value) => {
                resolve_native_background_size_value(value, custom_properties, 0)
            }
            LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => {
                Some(NativeBackgroundSize::default())
            }
            LocalCascadeDeclaration::RevertLayer => None,
        }
    })
}
