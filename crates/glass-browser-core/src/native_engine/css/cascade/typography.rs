use super::*;

pub(in crate::native_engine::css) fn resolve_text_align_declaration(
    declaration: InheritedTextDeclaration<TextAlignDeclarationValue>,
    inherited: TextAlignValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextAlignDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_text_align_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(TextAlignDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => {
            Some(TextAlignDeclarationValue::Value(TextAlignValue::Left))
        }
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_align_value(
    value: TextAlignDeclarationValue,
    inherited: TextAlignValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextAlignDeclarationValue> {
    match value {
        TextAlignDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_align_declaration(value))
                .and_then(|declaration| {
                    resolve_text_align_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        TextAlignDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_align_declaration(value))
                .and_then(|declaration| {
                    resolve_text_align_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(TextAlignDeclarationValue::Value(fallback)))
        }
        TextAlignDeclarationValue::CustomProperty(_)
        | TextAlignDeclarationValue::CustomPropertyFallback(_, _) => None,
        TextAlignDeclarationValue::Value(value) => Some(TextAlignDeclarationValue::Value(value)),
    }
}

pub(in crate::native_engine::css) fn resolve_text_align(
    candidates: NativeTextCascadeCandidates<TextAlignDeclarationValue>,
    inherited: TextAlignValue,
    custom_properties: &BTreeMap<u64, String>,
) -> TextAlignValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        TextAlignDeclarationValue::Value(inherited),
        |declaration| resolve_text_align_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        TextAlignDeclarationValue::Value(value) => value,
        TextAlignDeclarationValue::CustomProperty(_)
        | TextAlignDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_text_align_last_declaration(
    declaration: InheritedTextDeclaration<TextAlignLastDeclarationValue>,
    inherited: TextAlignLastValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextAlignLastDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_text_align_last_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(TextAlignLastDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(TextAlignLastDeclarationValue::Value(
            TextAlignLastValue::Auto,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_align_last_value(
    value: TextAlignLastDeclarationValue,
    inherited: TextAlignLastValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextAlignLastDeclarationValue> {
    match value {
        TextAlignLastDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_align_last_declaration(value))
                .and_then(|declaration| {
                    resolve_text_align_last_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        TextAlignLastDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_align_last_declaration(value))
                .and_then(|declaration| {
                    resolve_text_align_last_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(TextAlignLastDeclarationValue::Value(fallback)))
        }
        TextAlignLastDeclarationValue::CustomProperty(_)
        | TextAlignLastDeclarationValue::CustomPropertyFallback(_, _) => None,
        TextAlignLastDeclarationValue::Value(value) => {
            Some(TextAlignLastDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_text_align_last(
    candidates: NativeTextCascadeCandidates<TextAlignLastDeclarationValue>,
    inherited: TextAlignLastValue,
    custom_properties: &BTreeMap<u64, String>,
) -> TextAlignLastValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        TextAlignLastDeclarationValue::Value(inherited),
        |declaration| {
            resolve_text_align_last_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        TextAlignLastDeclarationValue::Value(value) => value,
        TextAlignLastDeclarationValue::CustomProperty(_)
        | TextAlignLastDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_text_justify_declaration(
    declaration: InheritedTextDeclaration<TextJustifyDeclarationValue>,
    inherited: TextJustifyValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextJustifyDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_text_justify_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(TextJustifyDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => {
            Some(TextJustifyDeclarationValue::Value(TextJustifyValue::Auto))
        }
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_justify_value(
    value: TextJustifyDeclarationValue,
    inherited: TextJustifyValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextJustifyDeclarationValue> {
    match value {
        TextJustifyDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_justify_declaration(value))
                .and_then(|declaration| {
                    resolve_text_justify_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        TextJustifyDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_justify_declaration(value))
                .and_then(|declaration| {
                    resolve_text_justify_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(TextJustifyDeclarationValue::Value(fallback)))
        }
        TextJustifyDeclarationValue::CustomProperty(_)
        | TextJustifyDeclarationValue::CustomPropertyFallback(_, _) => None,
        TextJustifyDeclarationValue::Value(value) => {
            Some(TextJustifyDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_text_justify(
    candidates: NativeTextCascadeCandidates<TextJustifyDeclarationValue>,
    inherited: TextJustifyValue,
    custom_properties: &BTreeMap<u64, String>,
) -> TextJustifyValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        TextJustifyDeclarationValue::Value(inherited),
        |declaration| {
            resolve_text_justify_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        TextJustifyDeclarationValue::Value(value) => value,
        TextJustifyDeclarationValue::CustomProperty(_)
        | TextJustifyDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn scale_relative_font_size(
    base: u32,
    scale_milli: u32,
) -> Option<u32> {
    let scaled = u64::from(base)
        .checked_mul(u64::from(scale_milli))?
        .checked_add(500)?
        .checked_div(1_000)?;
    let value = u32::try_from(scaled).ok()?;
    (1..=crate::native_engine::font::MAX_NATIVE_FONT_SIZE)
        .contains(&value)
        .then_some(value)
}

pub(in crate::native_engine::css) fn scale_viewport_font_size(
    viewport: Viewport,
    scale_milli: u32,
    unit: NativeViewportFontSizeUnit,
) -> Option<u32> {
    let base = match unit {
        NativeViewportFontSizeUnit::Width => viewport.width,
        NativeViewportFontSizeUnit::Height => viewport.height,
        NativeViewportFontSizeUnit::Min => viewport.width.min(viewport.height),
        NativeViewportFontSizeUnit::Max => viewport.width.max(viewport.height),
    };
    let scaled = u64::from(base)
        .checked_mul(u64::from(scale_milli))?
        .checked_add(50_000)?
        .checked_div(100_000)?;
    let value = u32::try_from(scaled).ok()?;
    (1..=crate::native_engine::font::MAX_NATIVE_FONT_SIZE)
        .contains(&value)
        .then_some(value)
}

pub(in crate::native_engine::css) fn resolve_font_size_calculation(
    calculation: NativeFontSizeCalculation,
    inherited: u32,
    root_font_size: u32,
    viewport: Viewport,
) -> Option<u32> {
    let mut total = calculation.absolute_milli.checked_mul(100)?;
    total = total.checked_add(
        i64::from(inherited)
            .checked_mul(calculation.parent_scale_milli)?
            .checked_mul(100)?,
    )?;
    total = total.checked_add(
        i64::from(root_font_size)
            .checked_mul(calculation.root_scale_milli)?
            .checked_mul(100)?,
    )?;
    total = total.checked_add(
        i64::from(viewport.width).checked_mul(calculation.viewport_width_scale_milli)?,
    )?;
    total = total.checked_add(
        i64::from(viewport.height).checked_mul(calculation.viewport_height_scale_milli)?,
    )?;
    let minimum = i64::from(viewport.width.min(viewport.height));
    total = total.checked_add(minimum.checked_mul(calculation.viewport_min_scale_milli)?)?;
    let maximum = i64::from(viewport.width.max(viewport.height));
    total = total.checked_add(maximum.checked_mul(calculation.viewport_max_scale_milli)?)?;
    if total <= 0 {
        return None;
    }
    let milli_pixels = total.checked_add(50)?.checked_div(100)?;
    let pixels = milli_pixels.checked_add(500)?.checked_div(1_000)?;
    let value = u32::try_from(pixels).ok()?;
    (1..=crate::native_engine::font::MAX_NATIVE_FONT_SIZE)
        .contains(&value)
        .then_some(value)
}

pub(in crate::native_engine::css) fn expand_font_size_variables(
    source: &str,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<String> {
    if !source.is_ascii() || source.len() > MAX_NATIVE_CUSTOM_PROPERTY_VALUE_BYTES {
        return None;
    }
    let bytes = source.as_bytes();
    let mut expanded = String::with_capacity(source.len());
    let mut cursor = 0_usize;
    while cursor < bytes.len() {
        let mut start = None;
        for index in cursor..bytes.len().saturating_sub(3) {
            if bytes
                .get(index..index.saturating_add(4))
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(b"var("))
            {
                start = Some(index);
                break;
            }
        }
        let Some(start) = start else {
            let literal = source.get(cursor..)?;
            if expanded.len().saturating_add(literal.len()) > MAX_NATIVE_CUSTOM_PROPERTY_VALUE_BYTES
            {
                return None;
            }
            expanded.push_str(literal);
            break;
        };
        let literal = source.get(cursor..start)?;
        if expanded.len().saturating_add(literal.len()) > MAX_NATIVE_CUSTOM_PROPERTY_VALUE_BYTES {
            return None;
        }
        expanded.push_str(literal);
        if depth >= MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH {
            return None;
        }
        let mut nesting = 1_usize;
        let mut close = None;
        for (index, byte) in bytes.iter().enumerate().skip(start.saturating_add(4)) {
            match *byte {
                b'(' => nesting = nesting.checked_add(1)?,
                b')' => {
                    nesting = nesting.checked_sub(1)?;
                    if nesting == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let arguments = source.get(start.saturating_add(4)..close)?;
        let (name, fallback) = parse_font_size_var_arguments(arguments)?;
        let name_hash = parse_custom_property_name(name)?;
        let mapped = custom_properties.get(&name_hash).map(String::as_str);
        let mut candidate = mapped
            .and_then(|replacement| {
                expand_font_size_variables(replacement, custom_properties, depth.saturating_add(1))
            })
            .filter(|candidate| parse_font_size_value(candidate).is_some());
        if candidate.is_none() {
            candidate = fallback.and_then(|replacement| {
                expand_font_size_variables(replacement, custom_properties, depth.saturating_add(1))
            });
        }
        let candidate = candidate?;
        if expanded.len().saturating_add(candidate.len()) > MAX_NATIVE_CUSTOM_PROPERTY_VALUE_BYTES {
            return None;
        }
        expanded.push_str(&candidate);
        cursor = close.saturating_add(1);
    }
    Some(expanded)
}

pub(in crate::native_engine::css) fn resolve_font_size_fallback(
    fallback: NativeFontSizeFallbackValue,
    inherited: u32,
    root_font_size: u32,
    viewport: Viewport,
) -> Option<u32> {
    match fallback {
        NativeFontSizeFallbackValue::Pixels(value) => (1
            ..=crate::native_engine::font::MAX_NATIVE_FONT_SIZE)
            .contains(&value)
            .then_some(value),
        NativeFontSizeFallbackValue::Relative(scale_milli) => {
            scale_relative_font_size(inherited, scale_milli)
        }
        NativeFontSizeFallbackValue::RootRelative(scale_milli) => {
            scale_relative_font_size(root_font_size, scale_milli)
        }
        NativeFontSizeFallbackValue::Viewport(scale_milli, unit) => {
            scale_viewport_font_size(viewport, scale_milli, unit)
        }
        NativeFontSizeFallbackValue::Calculation(calculation) => {
            resolve_font_size_calculation(calculation, inherited, root_font_size, viewport)
        }
        NativeFontSizeFallbackValue::Inherit
        | NativeFontSizeFallbackValue::Unset
        | NativeFontSizeFallbackValue::Revert => Some(inherited),
        NativeFontSizeFallbackValue::Initial => {
            Some(crate::native_engine::font::DEFAULT_NATIVE_FONT_SIZE)
        }
        NativeFontSizeFallbackValue::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_font_size_declaration(
    declaration: InheritedTextDeclaration<NativeFontSizeDeclarationValue>,
    inherited: u32,
    root_font_size: u32,
    viewport: Viewport,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<u32> {
    match declaration {
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::Pixels(value)) => (1
            ..=crate::native_engine::font::MAX_NATIVE_FONT_SIZE)
            .contains(&value)
            .then_some(value),
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::Relative(scale_milli)) => {
            scale_relative_font_size(inherited, scale_milli)
        }
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::RootRelative(
            scale_milli,
        )) => scale_relative_font_size(root_font_size, scale_milli),
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::Viewport(
            scale_milli,
            unit,
        )) => scale_viewport_font_size(viewport, scale_milli, unit),
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::Calculation(
            calculation,
        )) => resolve_font_size_calculation(calculation, inherited, root_font_size, viewport),
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::VariableCalculation(
            expression,
        )) if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH => {
            let expanded =
                expand_font_size_variables(expression.as_str(), custom_properties, depth)?;
            let calculation = parse_font_size_calculation(&expanded)?;
            resolve_font_size_calculation(calculation, inherited, root_font_size, viewport)
        }
        InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::CustomProperty(
            name_hash,
        )) if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH => custom_properties
            .get(&name_hash)
            .and_then(|value| parse_font_size_value(value))
            .and_then(|value| {
                resolve_font_size_declaration(
                    InheritedTextDeclaration::Value(value),
                    inherited,
                    root_font_size,
                    viewport,
                    custom_properties,
                    depth.saturating_add(1),
                )
            }),
        InheritedTextDeclaration::Value(
            NativeFontSizeDeclarationValue::CustomPropertyFallback(name_hash, fallback),
        ) if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH => custom_properties
            .get(&name_hash)
            .and_then(|value| parse_font_size_value(value))
            .and_then(|value| {
                resolve_font_size_declaration(
                    InheritedTextDeclaration::Value(value),
                    inherited,
                    root_font_size,
                    viewport,
                    custom_properties,
                    depth.saturating_add(1),
                )
            })
            .or_else(|| resolve_font_size_fallback(fallback, inherited, root_font_size, viewport)),
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(inherited),
        InheritedTextDeclaration::Initial => {
            Some(crate::native_engine::font::DEFAULT_NATIVE_FONT_SIZE)
        }
        InheritedTextDeclaration::RevertLayer
        | InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::CustomProperty(_))
        | InheritedTextDeclaration::Value(
            NativeFontSizeDeclarationValue::CustomPropertyFallback(_, _),
        )
        | InheritedTextDeclaration::Value(NativeFontSizeDeclarationValue::VariableCalculation(_)) => {
            None
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_size(
    candidates: NativeTextCascadeCandidates<NativeFontSizeDeclarationValue>,
    inherited: u32,
    root_font_size: u32,
    viewport: Viewport,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    resolve_alignment_candidates(candidates, inherited, |declaration| {
        resolve_font_size_declaration(
            declaration,
            inherited,
            root_font_size,
            viewport,
            custom_properties,
            0,
        )
    })
}

pub(in crate::native_engine::css) fn resolve_text_transform_declaration(
    declaration: InheritedTextDeclaration<TextTransformDeclarationValue>,
    inherited: TextTransformValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextTransformDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_text_transform_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(TextTransformDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(TextTransformDeclarationValue::Value(
            TextTransformValue::None,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_text_transform_value(
    value: TextTransformDeclarationValue,
    inherited: TextTransformValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextTransformDeclarationValue> {
    match value {
        TextTransformDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_transform_declaration(value))
                .and_then(|declaration| {
                    resolve_text_transform_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        TextTransformDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_transform_declaration(value))
                .and_then(|declaration| {
                    resolve_text_transform_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(TextTransformDeclarationValue::Value(fallback)))
        }
        TextTransformDeclarationValue::CustomProperty(_)
        | TextTransformDeclarationValue::CustomPropertyFallback(_, _) => None,
        TextTransformDeclarationValue::Value(value) => {
            Some(TextTransformDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_text_transform(
    candidates: NativeTextCascadeCandidates<TextTransformDeclarationValue>,
    inherited: TextTransformValue,
    custom_properties: &BTreeMap<u64, String>,
) -> TextTransformValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        TextTransformDeclarationValue::Value(inherited),
        |declaration| {
            resolve_text_transform_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        TextTransformDeclarationValue::Value(value) => value,
        TextTransformDeclarationValue::CustomProperty(_)
        | TextTransformDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_pointer_events_declaration(
    declaration: InheritedTextDeclaration<PointerEventsDeclarationValue>,
    inherited: NativePointerEventsValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PointerEventsDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_pointer_events_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(PointerEventsDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(PointerEventsDeclarationValue::Value(
            NativePointerEventsValue::Auto,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_pointer_events_value(
    value: PointerEventsDeclarationValue,
    inherited: NativePointerEventsValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PointerEventsDeclarationValue> {
    match value {
        PointerEventsDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_pointer_events_declaration(value))
                .and_then(|declaration| {
                    resolve_pointer_events_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        PointerEventsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_pointer_events_declaration(value))
                .and_then(|declaration| {
                    resolve_pointer_events_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(PointerEventsDeclarationValue::Value(fallback)))
        }
        PointerEventsDeclarationValue::CustomProperty(_)
        | PointerEventsDeclarationValue::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(in crate::native_engine::css) fn resolve_pointer_events(
    candidates: NativeTextCascadeCandidates<PointerEventsDeclarationValue>,
    inherited: NativePointerEventsValue,
    custom_properties: &BTreeMap<u64, String>,
) -> NativePointerEventsValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        PointerEventsDeclarationValue::Value(inherited),
        |declaration| {
            resolve_pointer_events_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        PointerEventsDeclarationValue::Value(value) => value,
        PointerEventsDeclarationValue::CustomProperty(_)
        | PointerEventsDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_numeric_declaration(
    declaration: InheritedTextDeclaration<FontVariantNumericDeclarationValue>,
    inherited: NativeFontVariantNumeric,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantNumericDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variant_numeric_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariantNumericDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariantNumericDeclarationValue::Value(
            NativeFontVariantNumeric::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_numeric_value(
    value: FontVariantNumericDeclarationValue,
    inherited: NativeFontVariantNumeric,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantNumericDeclarationValue> {
    match value {
        FontVariantNumericDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_numeric_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_numeric_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariantNumericDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_numeric_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_numeric_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariantNumericDeclarationValue::Value(fallback)))
        }
        FontVariantNumericDeclarationValue::CustomProperty(_)
        | FontVariantNumericDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariantNumericDeclarationValue::Value(value) => {
            Some(FontVariantNumericDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variant_numeric(
    candidates: NativeTextCascadeCandidates<FontVariantNumericDeclarationValue>,
    inherited: NativeFontVariantNumeric,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariantNumeric {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariantNumericDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variant_numeric_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariantNumericDeclarationValue::Value(value) => value,
        FontVariantNumericDeclarationValue::CustomProperty(_)
        | FontVariantNumericDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_east_asian_declaration(
    declaration: InheritedTextDeclaration<FontVariantEastAsianDeclarationValue>,
    inherited: NativeFontVariantEastAsian,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantEastAsianDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variant_east_asian_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariantEastAsianDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariantEastAsianDeclarationValue::Value(
            NativeFontVariantEastAsian::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_east_asian_value(
    value: FontVariantEastAsianDeclarationValue,
    inherited: NativeFontVariantEastAsian,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantEastAsianDeclarationValue> {
    match value {
        FontVariantEastAsianDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_east_asian_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_east_asian_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariantEastAsianDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_east_asian_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_east_asian_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariantEastAsianDeclarationValue::Value(fallback)))
        }
        FontVariantEastAsianDeclarationValue::CustomProperty(_)
        | FontVariantEastAsianDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariantEastAsianDeclarationValue::Value(value) => {
            Some(FontVariantEastAsianDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variant_east_asian(
    candidates: NativeTextCascadeCandidates<FontVariantEastAsianDeclarationValue>,
    inherited: NativeFontVariantEastAsian,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariantEastAsian {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariantEastAsianDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variant_east_asian_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariantEastAsianDeclarationValue::Value(value) => value,
        FontVariantEastAsianDeclarationValue::CustomProperty(_)
        | FontVariantEastAsianDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_alternates_declaration(
    declaration: InheritedTextDeclaration<FontVariantAlternatesDeclarationValue>,
    inherited: NativeFontVariantAlternates,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantAlternatesDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variant_alternates_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariantAlternatesDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariantAlternatesDeclarationValue::Value(
            NativeFontVariantAlternates::Normal,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_alternates_value(
    value: FontVariantAlternatesDeclarationValue,
    inherited: NativeFontVariantAlternates,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantAlternatesDeclarationValue> {
    match value {
        FontVariantAlternatesDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_alternates_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_alternates_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariantAlternatesDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_alternates_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_alternates_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariantAlternatesDeclarationValue::Value(fallback)))
        }
        FontVariantAlternatesDeclarationValue::CustomProperty(_)
        | FontVariantAlternatesDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariantAlternatesDeclarationValue::Value(value) => {
            Some(FontVariantAlternatesDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variant_alternates(
    candidates: NativeTextCascadeCandidates<FontVariantAlternatesDeclarationValue>,
    inherited: NativeFontVariantAlternates,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariantAlternates {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariantAlternatesDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variant_alternates_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariantAlternatesDeclarationValue::Value(value) => value,
        FontVariantAlternatesDeclarationValue::CustomProperty(_)
        | FontVariantAlternatesDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_position_declaration(
    declaration: InheritedTextDeclaration<FontVariantPositionDeclarationValue>,
    inherited: NativeFontVariantPosition,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantPositionDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variant_position_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariantPositionDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariantPositionDeclarationValue::Value(
            NativeFontVariantPosition::Normal,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_position_value(
    value: FontVariantPositionDeclarationValue,
    inherited: NativeFontVariantPosition,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantPositionDeclarationValue> {
    match value {
        FontVariantPositionDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_position_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_position_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariantPositionDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_position_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_position_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariantPositionDeclarationValue::Value(fallback)))
        }
        FontVariantPositionDeclarationValue::CustomProperty(_)
        | FontVariantPositionDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariantPositionDeclarationValue::Value(value) => {
            Some(FontVariantPositionDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variant_position(
    candidates: NativeTextCascadeCandidates<FontVariantPositionDeclarationValue>,
    inherited: NativeFontVariantPosition,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariantPosition {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariantPositionDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variant_position_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariantPositionDeclarationValue::Value(value) => value,
        FontVariantPositionDeclarationValue::CustomProperty(_)
        | FontVariantPositionDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_caps_declaration(
    declaration: InheritedTextDeclaration<FontVariantCapsDeclarationValue>,
    inherited: NativeFontVariantCaps,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantCapsDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variant_caps_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariantCapsDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariantCapsDeclarationValue::Value(
            NativeFontVariantCaps::Normal,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_caps_value(
    value: FontVariantCapsDeclarationValue,
    inherited: NativeFontVariantCaps,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantCapsDeclarationValue> {
    match value {
        FontVariantCapsDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_caps_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_caps_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariantCapsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_caps_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_caps_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariantCapsDeclarationValue::Value(fallback)))
        }
        FontVariantCapsDeclarationValue::CustomProperty(_)
        | FontVariantCapsDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariantCapsDeclarationValue::Value(value) => {
            Some(FontVariantCapsDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variant_caps(
    candidates: NativeTextCascadeCandidates<FontVariantCapsDeclarationValue>,
    inherited: NativeFontVariantCaps,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariantCaps {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariantCapsDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variant_caps_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariantCapsDeclarationValue::Value(value) => value,
        FontVariantCapsDeclarationValue::CustomProperty(_)
        | FontVariantCapsDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_ligatures_declaration(
    declaration: InheritedTextDeclaration<FontVariantLigaturesDeclarationValue>,
    inherited: NativeFontVariantLigatures,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantLigaturesDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variant_ligatures_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariantLigaturesDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariantLigaturesDeclarationValue::Value(
            NativeFontVariantLigatures::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variant_ligatures_value(
    value: FontVariantLigaturesDeclarationValue,
    inherited: NativeFontVariantLigatures,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariantLigaturesDeclarationValue> {
    match value {
        FontVariantLigaturesDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_ligatures_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_ligatures_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariantLigaturesDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variant_ligatures_custom_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variant_ligatures_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariantLigaturesDeclarationValue::Value(fallback)))
        }
        FontVariantLigaturesDeclarationValue::CustomProperty(_)
        | FontVariantLigaturesDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariantLigaturesDeclarationValue::Value(value) => {
            Some(FontVariantLigaturesDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variant_ligatures(
    candidates: NativeTextCascadeCandidates<FontVariantLigaturesDeclarationValue>,
    inherited: NativeFontVariantLigatures,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariantLigatures {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariantLigaturesDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variant_ligatures_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariantLigaturesDeclarationValue::Value(value) => value,
        FontVariantLigaturesDeclarationValue::CustomProperty(_)
        | FontVariantLigaturesDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_language_override_declaration(
    declaration: InheritedTextDeclaration<FontLanguageOverrideDeclarationValue>,
    inherited: NativeFontLanguageOverride,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontLanguageOverrideDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_language_override_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontLanguageOverrideDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontLanguageOverrideDeclarationValue::Value(
            NativeFontLanguageOverride::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_language_override_value(
    value: FontLanguageOverrideDeclarationValue,
    inherited: NativeFontLanguageOverride,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontLanguageOverrideDeclarationValue> {
    match value {
        FontLanguageOverrideDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_language_override_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_language_override_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontLanguageOverrideDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_language_override_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_language_override_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontLanguageOverrideDeclarationValue::Value(fallback)))
        }
        FontLanguageOverrideDeclarationValue::CustomProperty(_)
        | FontLanguageOverrideDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontLanguageOverrideDeclarationValue::Value(value) => {
            Some(FontLanguageOverrideDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_language_override(
    candidates: NativeTextCascadeCandidates<FontLanguageOverrideDeclarationValue>,
    inherited: NativeFontLanguageOverride,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontLanguageOverride {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontLanguageOverrideDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_language_override_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontLanguageOverrideDeclarationValue::Value(value) => value,
        FontLanguageOverrideDeclarationValue::CustomProperty(_)
        | FontLanguageOverrideDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variation_settings_declaration(
    declaration: InheritedTextDeclaration<FontVariationSettingsDeclarationValue>,
    inherited: NativeFontVariationSettings,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariationSettingsDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_variation_settings_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontVariationSettingsDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontVariationSettingsDeclarationValue::Value(
            NativeFontVariationSettings::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_variation_settings_value(
    value: FontVariationSettingsDeclarationValue,
    inherited: NativeFontVariationSettings,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontVariationSettingsDeclarationValue> {
    match value {
        FontVariationSettingsDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variation_settings_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variation_settings_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontVariationSettingsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_variation_settings_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_variation_settings_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontVariationSettingsDeclarationValue::Value(fallback)))
        }
        FontVariationSettingsDeclarationValue::CustomProperty(_)
        | FontVariationSettingsDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontVariationSettingsDeclarationValue::Value(value) => {
            Some(FontVariationSettingsDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_variation_settings(
    candidates: NativeTextCascadeCandidates<FontVariationSettingsDeclarationValue>,
    inherited: NativeFontVariationSettings,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontVariationSettings {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontVariationSettingsDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_variation_settings_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontVariationSettingsDeclarationValue::Value(value) => value,
        FontVariationSettingsDeclarationValue::CustomProperty(_)
        | FontVariationSettingsDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_feature_settings_declaration(
    declaration: InheritedTextDeclaration<FontFeatureSettingsDeclarationValue>,
    inherited: NativeFontFeatureSettings,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontFeatureSettingsDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_feature_settings_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontFeatureSettingsDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontFeatureSettingsDeclarationValue::Value(
            NativeFontFeatureSettings::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_feature_settings_value(
    value: FontFeatureSettingsDeclarationValue,
    inherited: NativeFontFeatureSettings,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontFeatureSettingsDeclarationValue> {
    match value {
        FontFeatureSettingsDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_feature_settings_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_feature_settings_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontFeatureSettingsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_feature_settings_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_feature_settings_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontFeatureSettingsDeclarationValue::Value(fallback)))
        }
        FontFeatureSettingsDeclarationValue::CustomProperty(_)
        | FontFeatureSettingsDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontFeatureSettingsDeclarationValue::Value(value) => {
            Some(FontFeatureSettingsDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_feature_settings(
    candidates: NativeTextCascadeCandidates<FontFeatureSettingsDeclarationValue>,
    inherited: NativeFontFeatureSettings,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontFeatureSettings {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontFeatureSettingsDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_feature_settings_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontFeatureSettingsDeclarationValue::Value(value) => value,
        FontFeatureSettingsDeclarationValue::CustomProperty(_)
        | FontFeatureSettingsDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_palette_declaration(
    declaration: InheritedTextDeclaration<FontPaletteDeclarationValue>,
    inherited: NativeFontPalette,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontPaletteDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_palette_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(FontPaletteDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(FontPaletteDeclarationValue::Value(
            NativeFontPalette::Normal,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_palette_value(
    value: FontPaletteDeclarationValue,
    inherited: NativeFontPalette,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontPaletteDeclarationValue> {
    match value {
        FontPaletteDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_palette_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_palette_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontPaletteDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_palette_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_palette_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontPaletteDeclarationValue::Value(fallback)))
        }
        FontPaletteDeclarationValue::CustomProperty(_)
        | FontPaletteDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontPaletteDeclarationValue::Value(value) => {
            Some(FontPaletteDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_palette(
    candidates: NativeTextCascadeCandidates<FontPaletteDeclarationValue>,
    inherited: NativeFontPalette,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontPalette {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontPaletteDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_palette_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        FontPaletteDeclarationValue::Value(value) => value,
        FontPaletteDeclarationValue::CustomProperty(_)
        | FontPaletteDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_optical_sizing_declaration(
    declaration: InheritedTextDeclaration<FontOpticalSizingDeclarationValue>,
    inherited: NativeFontOpticalSizing,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontOpticalSizingDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_optical_sizing_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => {
            Some(FontOpticalSizingDeclarationValue::Value(inherited))
        }
        InheritedTextDeclaration::Initial => Some(FontOpticalSizingDeclarationValue::Value(
            NativeFontOpticalSizing::Auto,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_optical_sizing_value(
    value: FontOpticalSizingDeclarationValue,
    inherited: NativeFontOpticalSizing,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontOpticalSizingDeclarationValue> {
    match value {
        FontOpticalSizingDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_optical_sizing_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_optical_sizing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontOpticalSizingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_optical_sizing_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_optical_sizing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontOpticalSizingDeclarationValue::Value(fallback)))
        }
        FontOpticalSizingDeclarationValue::CustomProperty(_)
        | FontOpticalSizingDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontOpticalSizingDeclarationValue::Value(value) => {
            Some(FontOpticalSizingDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_optical_sizing(
    candidates: NativeTextCascadeCandidates<FontOpticalSizingDeclarationValue>,
    inherited: NativeFontOpticalSizing,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontOpticalSizing {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontOpticalSizingDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_optical_sizing_declaration(
                declaration,
                inherited,
                custom_properties,
                0,
            )
        },
    );
    match resolved {
        FontOpticalSizingDeclarationValue::Value(value) => value,
        FontOpticalSizingDeclarationValue::CustomProperty(_)
        | FontOpticalSizingDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_kerning_declaration(
    declaration: InheritedTextDeclaration<FontKerningDeclarationValue>,
    inherited: NativeFontKerning,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontKerningDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_kerning_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(FontKerningDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => {
            Some(FontKerningDeclarationValue::Value(NativeFontKerning::Auto))
        }
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_kerning_value(
    value: FontKerningDeclarationValue,
    inherited: NativeFontKerning,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontKerningDeclarationValue> {
    match value {
        FontKerningDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_kerning_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_kerning_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontKerningDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_kerning_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_kerning_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontKerningDeclarationValue::Value(fallback)))
        }
        FontKerningDeclarationValue::CustomProperty(_)
        | FontKerningDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontKerningDeclarationValue::Value(value) => {
            Some(FontKerningDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_kerning(
    candidates: NativeTextCascadeCandidates<FontKerningDeclarationValue>,
    inherited: NativeFontKerning,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontKerning {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontKerningDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_kerning_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        FontKerningDeclarationValue::Value(value) => value,
        FontKerningDeclarationValue::CustomProperty(_)
        | FontKerningDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_family_declaration(
    declaration: InheritedTextDeclaration<FontFamilyDeclarationValue>,
    inherited: NativeFontFamilyList,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontFamilyDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_family_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(FontFamilyDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(FontFamilyDeclarationValue::Value(
            NativeFontFamilyList::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_family_value(
    value: FontFamilyDeclarationValue,
    inherited: NativeFontFamilyList,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontFamilyDeclarationValue> {
    match value {
        FontFamilyDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_family_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_family_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontFamilyDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_family_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_family_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontFamilyDeclarationValue::Value(fallback)))
        }
        FontFamilyDeclarationValue::CustomProperty(_)
        | FontFamilyDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontFamilyDeclarationValue::Value(value) => Some(FontFamilyDeclarationValue::Value(value)),
    }
}

pub(in crate::native_engine::css) fn resolve_font_family(
    candidates: NativeTextCascadeCandidates<FontFamilyDeclarationValue>,
    inherited: NativeFontFamilyList,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontFamilyList {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontFamilyDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_family_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        FontFamilyDeclarationValue::Value(value) => value,
        FontFamilyDeclarationValue::CustomProperty(_)
        | FontFamilyDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_stretch_declaration(
    declaration: InheritedTextDeclaration<FontStretchDeclarationValue>,
    inherited: NativeFontStretchRange,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontStretchDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_stretch_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(FontStretchDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(FontStretchDeclarationValue::Value(
            NativeFontStretchRange::default(),
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_stretch_value(
    value: FontStretchDeclarationValue,
    inherited: NativeFontStretchRange,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontStretchDeclarationValue> {
    match value {
        FontStretchDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_stretch_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_stretch_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontStretchDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_stretch_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_stretch_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontStretchDeclarationValue::Value(fallback)))
        }
        FontStretchDeclarationValue::CustomProperty(_)
        | FontStretchDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontStretchDeclarationValue::Value(value) => {
            Some(FontStretchDeclarationValue::Value(value))
        }
    }
}

pub(in crate::native_engine::css) fn resolve_font_stretch(
    candidates: NativeTextCascadeCandidates<FontStretchDeclarationValue>,
    inherited: NativeFontStretchRange,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeFontStretchRange {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontStretchDeclarationValue::Value(inherited),
        |declaration| {
            resolve_native_font_stretch_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        FontStretchDeclarationValue::Value(value) => value,
        FontStretchDeclarationValue::CustomProperty(_)
        | FontStretchDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(in crate::native_engine::css) fn font_style_declaration_value(
    value: FontStyleValue,
) -> FontStyleDeclarationValue {
    match value {
        FontStyleValue::Normal => FontStyleDeclarationValue::Normal,
        FontStyleValue::Italic => FontStyleDeclarationValue::Italic,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_style_declaration(
    declaration: InheritedTextDeclaration<FontStyleDeclarationValue>,
    inherited: FontStyleValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontStyleDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_style_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(font_style_declaration_value(inherited)),
        InheritedTextDeclaration::Initial => Some(FontStyleDeclarationValue::Normal),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_style_value(
    value: FontStyleDeclarationValue,
    inherited: FontStyleValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontStyleDeclarationValue> {
    match value {
        FontStyleDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_style_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_style_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontStyleDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_style_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_style_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or_else(|| Some(font_style_declaration_value(fallback)))
        }
        FontStyleDeclarationValue::CustomProperty(_)
        | FontStyleDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontStyleDeclarationValue::Normal => Some(FontStyleDeclarationValue::Normal),
        FontStyleDeclarationValue::Italic => Some(FontStyleDeclarationValue::Italic),
    }
}

pub(in crate::native_engine::css) fn resolve_font_style(
    candidates: NativeTextCascadeCandidates<FontStyleDeclarationValue>,
    inherited: FontStyleValue,
    custom_properties: &BTreeMap<u64, String>,
) -> FontStyleValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        font_style_declaration_value(inherited),
        |declaration| {
            resolve_native_font_style_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        FontStyleDeclarationValue::Normal => FontStyleValue::Normal,
        FontStyleDeclarationValue::Italic => FontStyleValue::Italic,
        FontStyleDeclarationValue::CustomProperty(_)
        | FontStyleDeclarationValue::CustomPropertyFallback(_, _) => FontStyleValue::Normal,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_weight_declaration(
    declaration: InheritedTextDeclaration<FontWeightDeclarationValue>,
    inherited: FontWeightValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontWeightDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_native_font_weight_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(FontWeightDeclarationValue::Absolute(inherited)),
        InheritedTextDeclaration::Initial => Some(FontWeightDeclarationValue::Absolute(
            FontWeightValue::Normal,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(in crate::native_engine::css) fn resolve_native_font_weight_value(
    value: FontWeightDeclarationValue,
    inherited: FontWeightValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<FontWeightDeclarationValue> {
    match value {
        FontWeightDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_weight_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_weight_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        FontWeightDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_font_weight_declaration(value))
                .and_then(|declaration| {
                    resolve_native_font_weight_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(FontWeightDeclarationValue::Absolute(fallback)))
        }
        FontWeightDeclarationValue::CustomProperty(_)
        | FontWeightDeclarationValue::CustomPropertyFallback(_, _) => None,
        FontWeightDeclarationValue::Absolute(value) => {
            Some(FontWeightDeclarationValue::Absolute(value))
        }
        FontWeightDeclarationValue::Lighter => Some(FontWeightDeclarationValue::Absolute(
            relative_font_weight(inherited, false),
        )),
        FontWeightDeclarationValue::Bolder => Some(FontWeightDeclarationValue::Absolute(
            relative_font_weight(inherited, true),
        )),
    }
}

pub(in crate::native_engine::css) fn resolve_font_weight(
    candidates: NativeTextCascadeCandidates<FontWeightDeclarationValue>,
    inherited: FontWeightValue,
    custom_properties: &BTreeMap<u64, String>,
) -> FontWeightValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        FontWeightDeclarationValue::Absolute(inherited),
        |declaration| {
            resolve_native_font_weight_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        FontWeightDeclarationValue::Absolute(value) => value,
        FontWeightDeclarationValue::Lighter | FontWeightDeclarationValue::Bolder => {
            FontWeightValue::Normal
        }
        FontWeightDeclarationValue::CustomProperty(_)
        | FontWeightDeclarationValue::CustomPropertyFallback(_, _) => FontWeightValue::Normal,
    }
}

pub(in crate::native_engine::css) fn relative_font_weight(
    inherited: FontWeightValue,
    bolder: bool,
) -> FontWeightValue {
    let weight = inherited.numeric();
    let resolved = if bolder {
        if weight < 350 {
            400
        } else if weight < 550 {
            700
        } else if weight < 900 {
            900
        } else {
            weight
        }
    } else if weight < 100 {
        weight
    } else if weight < 550 {
        100
    } else if weight < 750 {
        400
    } else {
        700
    };
    FontWeightValue::from_numeric(resolved).unwrap_or(FontWeightValue::Normal)
}
