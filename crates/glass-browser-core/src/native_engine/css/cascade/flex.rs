use super::*;

pub(in crate::native_engine::css) fn resolve_flex_direction_value(
    value: FlexDirectionDeclaration,
    inherited: FlexDirectionValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FlexDirectionValue> {
    match value {
        FlexDirectionDeclaration::Value(value) => Some(value),
        FlexDirectionDeclaration::Inherit => Some(inherited),
        FlexDirectionDeclaration::Reset => Some(FlexDirectionValue::Row),
        FlexDirectionDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_direction_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_direction_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexDirectionDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_direction_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_direction_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexDirectionDeclaration::CustomPropertyFlow(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_flow_declaration(value))
                .and_then(|(direction, _)| {
                    resolve_flex_direction_value(
                        direction,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexDirectionDeclaration::CustomPropertyFlowFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_flow_declaration(value))
                .and_then(|(direction, _)| {
                    resolve_flex_direction_value(
                        direction,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexDirectionDeclaration::CustomProperty(_)
        | FlexDirectionDeclaration::CustomPropertyFallback(_, _)
        | FlexDirectionDeclaration::CustomPropertyFlow(_)
        | FlexDirectionDeclaration::CustomPropertyFlowFallback(_, _)
        | FlexDirectionDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_flex_wrap_value(
    value: FlexWrapDeclaration,
    inherited: FlexWrapValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FlexWrapValue> {
    match value {
        FlexWrapDeclaration::Value(value) => Some(value),
        FlexWrapDeclaration::Inherit => Some(inherited),
        FlexWrapDeclaration::Reset => Some(FlexWrapValue::NoWrap),
        FlexWrapDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_wrap_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_wrap_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexWrapDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_wrap_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_wrap_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexWrapDeclaration::CustomPropertyFlow(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_flow_declaration(value))
                .and_then(|(_, wrap)| {
                    resolve_flex_wrap_value(
                        wrap,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexWrapDeclaration::CustomPropertyFlowFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_flow_declaration(value))
                .and_then(|(_, wrap)| {
                    resolve_flex_wrap_value(
                        wrap,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexWrapDeclaration::CustomProperty(_)
        | FlexWrapDeclaration::CustomPropertyFallback(_, _)
        | FlexWrapDeclaration::CustomPropertyFlow(_)
        | FlexWrapDeclaration::CustomPropertyFlowFallback(_, _)
        | FlexWrapDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_flex_direction(
    candidates: [Option<CascadeValue<FlexDirectionDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: FlexDirectionValue,
    custom_properties: &BTreeMap<u64, String>,
) -> FlexDirectionValue {
    resolve_alignment_candidates(candidates, FlexDirectionValue::Row, |declaration| {
        resolve_flex_direction_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_justify_content_value(
    value: JustifyContentDeclaration,
    inherited: JustifyContentValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<JustifyContentValue> {
    match value {
        JustifyContentDeclaration::Value(value) => Some(value),
        JustifyContentDeclaration::Inherit => Some(inherited),
        JustifyContentDeclaration::Reset => Some(JustifyContentValue::FlexStart),
        JustifyContentDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_justify_content_declaration(value))
                .and_then(|declaration| {
                    resolve_justify_content_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        JustifyContentDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_justify_content_declaration(value))
                .and_then(|declaration| {
                    resolve_justify_content_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        JustifyContentDeclaration::CustomPropertyPlaceContent(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_place_content_declaration(value))
                .and_then(|(_, justify_content)| {
                    resolve_justify_content_value(
                        justify_content,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        JustifyContentDeclaration::CustomPropertyPlaceContentFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_place_content_declaration(value))
                .and_then(|(_, justify_content)| {
                    resolve_justify_content_value(
                        justify_content,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        JustifyContentDeclaration::CustomProperty(_)
        | JustifyContentDeclaration::CustomPropertyFallback(_, _)
        | JustifyContentDeclaration::CustomPropertyPlaceContent(_)
        | JustifyContentDeclaration::CustomPropertyPlaceContentFallback(_, _)
        | JustifyContentDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_align_items_value(
    value: AlignItemsDeclaration,
    inherited: AlignItemsValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<AlignItemsValue> {
    match value {
        AlignItemsDeclaration::Value(value) => Some(value),
        AlignItemsDeclaration::Inherit => Some(inherited),
        AlignItemsDeclaration::Reset => Some(AlignItemsValue::FlexStart),
        AlignItemsDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_align_items_declaration(value))
                .and_then(|declaration| {
                    resolve_align_items_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        AlignItemsDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_align_items_declaration(value))
                .and_then(|declaration| {
                    resolve_align_items_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        AlignItemsDeclaration::CustomProperty(_)
        | AlignItemsDeclaration::CustomPropertyFallback(_, _)
        | AlignItemsDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_align_self_value(
    value: AlignSelfDeclaration,
    inherited: AlignSelfValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<AlignSelfValue> {
    match value {
        AlignSelfDeclaration::Value(value) => Some(value),
        AlignSelfDeclaration::Inherit => Some(inherited),
        AlignSelfDeclaration::Reset => Some(AlignSelfValue::Auto),
        AlignSelfDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_align_self_declaration(value))
                .and_then(|declaration| {
                    resolve_align_self_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        AlignSelfDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_align_self_declaration(value))
                .and_then(|declaration| {
                    resolve_align_self_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        AlignSelfDeclaration::CustomProperty(_)
        | AlignSelfDeclaration::CustomPropertyFallback(_, _)
        | AlignSelfDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_align_content_value(
    value: AlignContentDeclaration,
    inherited: AlignContentValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<AlignContentValue> {
    match value {
        AlignContentDeclaration::Value(value) => Some(value),
        AlignContentDeclaration::Inherit => Some(inherited),
        AlignContentDeclaration::Reset => Some(AlignContentValue::FlexStart),
        AlignContentDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_align_content_declaration(value))
                .and_then(|declaration| {
                    resolve_align_content_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        AlignContentDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_align_content_declaration(value))
                .and_then(|declaration| {
                    resolve_align_content_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        AlignContentDeclaration::CustomPropertyPlaceContent(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_place_content_declaration(value))
                .and_then(|(align_content, _)| {
                    resolve_align_content_value(
                        align_content,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        AlignContentDeclaration::CustomPropertyPlaceContentFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_place_content_declaration(value))
                .and_then(|(align_content, _)| {
                    resolve_align_content_value(
                        align_content,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        AlignContentDeclaration::CustomProperty(_)
        | AlignContentDeclaration::CustomPropertyFallback(_, _)
        | AlignContentDeclaration::CustomPropertyPlaceContent(_)
        | AlignContentDeclaration::CustomPropertyPlaceContentFallback(_, _)
        | AlignContentDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_justify_content(
    candidates: [Option<CascadeValue<JustifyContentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: JustifyContentValue,
    custom_properties: &BTreeMap<u64, String>,
) -> JustifyContentValue {
    resolve_alignment_candidates(candidates, JustifyContentValue::FlexStart, |declaration| {
        resolve_justify_content_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_align_items(
    candidates: [Option<CascadeValue<AlignItemsDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: AlignItemsValue,
    custom_properties: &BTreeMap<u64, String>,
) -> AlignItemsValue {
    resolve_alignment_candidates(candidates, AlignItemsValue::FlexStart, |declaration| {
        resolve_align_items_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_align_self(
    candidates: [Option<CascadeValue<AlignSelfDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: AlignSelfValue,
    custom_properties: &BTreeMap<u64, String>,
) -> AlignSelfValue {
    resolve_alignment_candidates(candidates, AlignSelfValue::Auto, |declaration| {
        resolve_align_self_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_align_content(
    candidates: [Option<CascadeValue<AlignContentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: AlignContentValue,
    custom_properties: &BTreeMap<u64, String>,
) -> AlignContentValue {
    resolve_alignment_candidates(candidates, AlignContentValue::FlexStart, |declaration| {
        resolve_align_content_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_flex_wrap(
    candidates: [Option<CascadeValue<FlexWrapDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: FlexWrapValue,
    custom_properties: &BTreeMap<u64, String>,
) -> FlexWrapValue {
    resolve_alignment_candidates(candidates, FlexWrapValue::NoWrap, |declaration| {
        resolve_flex_wrap_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_flex_item_order_value(
    value: FlexItemOrderDeclaration,
    inherited: NativeOrderValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeOrderValue> {
    match value {
        FlexItemOrderDeclaration::Value(value) => Some(value),
        FlexItemOrderDeclaration::Inherit => Some(inherited),
        FlexItemOrderDeclaration::Reset => Some(NativeOrderValue::default()),
        FlexItemOrderDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_item_order_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_item_order_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexItemOrderDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_item_order_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_item_order_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexItemOrderDeclaration::CustomProperty(_)
        | FlexItemOrderDeclaration::CustomPropertyFallback(_, _)
        | FlexItemOrderDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_flex_item_order(
    candidates: [Option<CascadeValue<FlexItemOrderDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: NativeOrderValue,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeOrderValue {
    resolve_alignment_candidates(candidates, NativeOrderValue::default(), |declaration| {
        resolve_flex_item_order_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_flex_grow_value(
    value: FlexGrowDeclaration,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match value {
        FlexGrowDeclaration::Value(value) => Some(value),
        FlexGrowDeclaration::Inherit => Some(inherited),
        FlexGrowDeclaration::Reset => Some(0),
        FlexGrowDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_grow_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_grow_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexGrowDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_grow_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_grow_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexGrowDeclaration::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shorthand_declaration(value))
                .and_then(|(grow, _, _)| {
                    resolve_flex_grow_value(
                        grow,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexGrowDeclaration::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shorthand_declaration(value))
                .and_then(|(grow, _, _)| {
                    resolve_flex_grow_value(
                        grow,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexGrowDeclaration::CustomProperty(_)
        | FlexGrowDeclaration::CustomPropertyFallback(_, _)
        | FlexGrowDeclaration::CustomPropertyShorthand(_)
        | FlexGrowDeclaration::CustomPropertyShorthandFallback(_, _)
        | FlexGrowDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_flex_shrink_value(
    value: FlexShrinkDeclaration,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match value {
        FlexShrinkDeclaration::Value(value) => Some(value),
        FlexShrinkDeclaration::Inherit => Some(inherited),
        FlexShrinkDeclaration::Reset => Some(1),
        FlexShrinkDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shrink_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_shrink_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexShrinkDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shrink_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_shrink_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexShrinkDeclaration::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shorthand_declaration(value))
                .and_then(|(_, shrink, _)| {
                    resolve_flex_shrink_value(
                        shrink,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexShrinkDeclaration::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shorthand_declaration(value))
                .and_then(|(_, shrink, _)| {
                    resolve_flex_shrink_value(
                        shrink,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexShrinkDeclaration::CustomProperty(_)
        | FlexShrinkDeclaration::CustomPropertyFallback(_, _)
        | FlexShrinkDeclaration::CustomPropertyShorthand(_)
        | FlexShrinkDeclaration::CustomPropertyShorthandFallback(_, _)
        | FlexShrinkDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_flex_basis_value(
    value: FlexBasisDeclaration,
    inherited: FlexBasisValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FlexBasisValue> {
    match value {
        FlexBasisDeclaration::Value(value) => Some(value),
        FlexBasisDeclaration::Inherit => Some(inherited),
        FlexBasisDeclaration::Reset => Some(FlexBasisValue::Auto),
        FlexBasisDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_basis_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_basis_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexBasisDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_basis_declaration(value))
                .and_then(|declaration| {
                    resolve_flex_basis_value(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexBasisDeclaration::CustomPropertyShorthand(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shorthand_declaration(value))
                .and_then(|(_, _, basis)| {
                    resolve_flex_basis_value(
                        basis,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FlexBasisDeclaration::CustomPropertyShorthandFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_flex_shorthand_declaration(value))
                .and_then(|(_, _, basis)| {
                    resolve_flex_basis_value(
                        basis,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        FlexBasisDeclaration::CustomProperty(_)
        | FlexBasisDeclaration::CustomPropertyFallback(_, _)
        | FlexBasisDeclaration::CustomPropertyShorthand(_)
        | FlexBasisDeclaration::CustomPropertyShorthandFallback(_, _)
        | FlexBasisDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_flex_grow(
    candidates: [Option<CascadeValue<FlexGrowDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    resolve_alignment_candidates(candidates, 0, |declaration| {
        resolve_flex_grow_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_flex_shrink(
    candidates: [Option<CascadeValue<FlexShrinkDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    resolve_alignment_candidates(candidates, 1, |declaration| {
        resolve_flex_shrink_value(declaration, inherited, custom_properties, 0)
    })
}

pub(in crate::native_engine::css) fn resolve_flex_basis(
    candidates: [Option<CascadeValue<FlexBasisDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    inherited: FlexBasisValue,
    custom_properties: &BTreeMap<u64, String>,
) -> FlexBasisValue {
    resolve_alignment_candidates(candidates, FlexBasisValue::Auto, |declaration| {
        resolve_flex_basis_value(declaration, inherited, custom_properties, 0)
    })
}
