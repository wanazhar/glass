use super::*;

pub(in crate::native_engine::css) fn parse_font_size(value: &str) -> Option<u32> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(number) = value.strip_suffix("px") {
        let milli_pixels = u64::from(parse_font_size_decimal_milli(number.trim())?);
        let pixels = milli_pixels.checked_add(500)?.checked_div(1_000)?;
        return u32::try_from(pixels)
            .ok()
            .filter(|value| (1..=256).contains(value));
    }
    let (number, multiplier, denominator) = [
        ("pt", 4_u64, 3_u64),
        ("pc", 16_u64, 1_u64),
        ("in", 96_u64, 1_u64),
        ("cm", 4_800_u64, 127_u64),
        ("mm", 480_u64, 127_u64),
    ]
    .iter()
    .find_map(|(suffix, multiplier, denominator)| {
        value
            .strip_suffix(suffix)
            .map(|number| (number.trim(), *multiplier, *denominator))
    })?;
    let milli_units = u64::from(parse_font_size_decimal_milli(number)?);
    let milli_pixels = milli_units
        .checked_mul(multiplier)?
        .checked_add(denominator / 2)?
        .checked_div(denominator)?;
    let pixels = milli_pixels.checked_add(999)?.checked_div(1_000)?;
    u32::try_from(pixels)
        .ok()
        .filter(|value| (1..=256).contains(value))
}
pub(in crate::native_engine::css) fn parse_relative_font_size(
    value: &str,
) -> Option<NativeFontSizeDeclarationValue> {
    let value = value.trim().to_ascii_lowercase();
    let (number, root_relative, percentage, viewport_unit) =
        if let Some(number) = value.strip_suffix('%') {
            (number.trim(), false, true, None)
        } else if let Some(number) = value.strip_suffix("rem") {
            (number.trim(), true, false, None)
        } else if let Some(number) = value.strip_suffix("vmin") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Min),
            )
        } else if let Some(number) = value.strip_suffix("vmax") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Max),
            )
        } else if let Some(number) = value.strip_suffix("vw") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Width),
            )
        } else if let Some(number) = value.strip_suffix("vh") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Height),
            )
        } else {
            let number = value.strip_suffix("em")?;
            (number.trim(), false, false, None)
        };
    let milli = parse_font_size_decimal_milli(number)?;
    let scale_milli = if percentage {
        milli.checked_add(50)?.checked_div(100)?
    } else {
        milli
    };
    if !(1..=crate::native_engine::font::MAX_NATIVE_FONT_SIZE.saturating_mul(1_000))
        .contains(&scale_milli)
    {
        return None;
    }
    Some(if let Some(unit) = viewport_unit {
        NativeFontSizeDeclarationValue::Viewport(scale_milli, unit)
    } else if root_relative {
        NativeFontSizeDeclarationValue::RootRelative(scale_milli)
    } else {
        NativeFontSizeDeclarationValue::Relative(scale_milli)
    })
}

pub(in crate::native_engine::css) const MAX_NATIVE_FONT_SIZE_CALC_TERMS: usize = 16;
pub(in crate::native_engine::css) const MAX_NATIVE_FONT_SIZE_CALC_PRODUCT_OPERANDS: usize = 8;

pub(in crate::native_engine::css) fn merge_font_size_calculation(
    total: &mut NativeFontSizeCalculation,
    term: NativeFontSizeCalculation,
    sign: i64,
) -> Option<()> {
    total.absolute_milli = total
        .absolute_milli
        .checked_add(sign.checked_mul(term.absolute_milli)?)?;
    total.parent_scale_milli = total
        .parent_scale_milli
        .checked_add(sign.checked_mul(term.parent_scale_milli)?)?;
    total.root_scale_milli = total
        .root_scale_milli
        .checked_add(sign.checked_mul(term.root_scale_milli)?)?;
    total.viewport_width_scale_milli = total
        .viewport_width_scale_milli
        .checked_add(sign.checked_mul(term.viewport_width_scale_milli)?)?;
    total.viewport_height_scale_milli = total
        .viewport_height_scale_milli
        .checked_add(sign.checked_mul(term.viewport_height_scale_milli)?)?;
    total.viewport_min_scale_milli = total
        .viewport_min_scale_milli
        .checked_add(sign.checked_mul(term.viewport_min_scale_milli)?)?;
    total.viewport_max_scale_milli = total
        .viewport_max_scale_milli
        .checked_add(sign.checked_mul(term.viewport_max_scale_milli)?)?;
    Some(())
}

pub(in crate::native_engine::css) fn parse_font_size_calculation_term(
    value: &str,
) -> Option<NativeFontSizeCalculation> {
    let value = value.trim();
    let mut term = NativeFontSizeCalculation::default();
    if let Some(number) = value.strip_suffix("px") {
        let milli = parse_font_size_decimal_milli(number.trim())?;
        (milli <= crate::native_engine::font::MAX_NATIVE_FONT_SIZE.saturating_mul(1_000))
            .then_some(())?;
        term.absolute_milli = i64::from(milli);
        return Some(term);
    }
    let (number, multiplier, denominator) = [
        ("pt", 4_u64, 3_u64),
        ("pc", 16_u64, 1_u64),
        ("in", 96_u64, 1_u64),
        ("cm", 4_800_u64, 127_u64),
        ("mm", 480_u64, 127_u64),
    ]
    .iter()
    .find_map(|(suffix, multiplier, denominator)| {
        value
            .strip_suffix(suffix)
            .map(|number| (number.trim(), *multiplier, *denominator))
    })?;
    let milli_units = u64::from(parse_font_size_decimal_milli(number)?);
    let milli_pixels = milli_units
        .checked_mul(multiplier)?
        .checked_add(denominator / 2)?
        .checked_div(denominator)?;
    if milli_pixels
        > u64::from(crate::native_engine::font::MAX_NATIVE_FONT_SIZE).saturating_mul(1_000)
    {
        return None;
    }
    term.absolute_milli = i64::try_from(milli_pixels).ok()?;
    Some(term)
}

pub(in crate::native_engine::css) fn parse_font_size_calculation_relative_term(
    value: &str,
) -> Option<(u32, Option<NativeViewportFontSizeUnit>, bool)> {
    let (number, root_relative, percentage, viewport_unit) =
        if let Some(number) = value.strip_suffix('%') {
            (number.trim(), false, true, None)
        } else if let Some(number) = value.strip_suffix("rem") {
            (number.trim(), true, false, None)
        } else if let Some(number) = value.strip_suffix("vmin") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Min),
            )
        } else if let Some(number) = value.strip_suffix("vmax") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Max),
            )
        } else if let Some(number) = value.strip_suffix("vw") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Width),
            )
        } else if let Some(number) = value.strip_suffix("vh") {
            (
                number.trim(),
                false,
                false,
                Some(NativeViewportFontSizeUnit::Height),
            )
        } else {
            let number = value.strip_suffix("em")?;
            (number.trim(), false, false, None)
        };
    let milli = parse_font_size_decimal_milli(number)?;
    let scale_milli = if percentage {
        milli.checked_add(50)?.checked_div(100)?
    } else {
        milli
    };
    (scale_milli <= crate::native_engine::font::MAX_NATIVE_FONT_SIZE.saturating_mul(1_000))
        .then_some(())?;
    Some((scale_milli, viewport_unit, root_relative))
}

pub(in crate::native_engine::css) fn parse_dimension_custom_property(
    value: &str,
) -> Option<DimensionDeclarationValue> {
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
        Some(fallback) => parse_dimension(fallback)
            .map(|fallback| DimensionDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(DimensionDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_dimension_property(
    value: &str,
) -> Option<DimensionDeclarationValue> {
    parse_dimension(value)
        .map(DimensionDeclarationValue::Value)
        .or_else(|| parse_dimension_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_local_dimension_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<DimensionDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_dimension_property)
}

pub(in crate::native_engine::css) fn parse_padding_var_arguments(
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

pub(in crate::native_engine::css) fn parse_padding_custom_property(
    value: &str,
) -> Option<PaddingDeclarationValue> {
    let (name_hash, fallback) = parse_padding_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_dimension(fallback)
            .map(|fallback| PaddingDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(PaddingDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_padding_shorthand_custom_property(
    value: &str,
) -> Option<(u64, Option<[u32; 4]>)> {
    let (name_hash, fallback) = parse_padding_var_arguments(value)?;
    let fallback = match fallback {
        Some(fallback) => Some(parse_box_edges(fallback)?),
        None => None,
    };
    Some((name_hash, fallback))
}

pub(in crate::native_engine::css) fn parse_padding_property(
    value: &str,
) -> Option<PaddingDeclarationValue> {
    parse_dimension(value)
        .map(PaddingDeclarationValue::Value)
        .or_else(|| parse_padding_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_local_physical_padding_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<PaddingDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(
            PaddingDeclarationValue::Value(0),
        ));
    }
    parse_local_cascade_declaration(value, parse_padding_property)
}

pub(in crate::native_engine::css) fn parse_local_padding_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<u32>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(0));
    }
    parse_local_cascade_declaration(value, parse_dimension)
}

pub(in crate::native_engine::css) fn parse_margin_var_arguments(
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

pub(in crate::native_engine::css) fn parse_margin_custom_property(
    value: &str,
) -> Option<MarginDeclarationValue> {
    let (name_hash, fallback) = parse_margin_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_margin_value(fallback)
            .map(|fallback| MarginDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(MarginDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_margin_shorthand_custom_property(
    value: &str,
) -> Option<(u64, Option<[NativeMarginValue; 4]>)> {
    let (name_hash, fallback) = parse_margin_var_arguments(value)?;
    let fallback = match fallback {
        Some(fallback) => Some(parse_margin_edges(fallback)?),
        None => None,
    };
    Some((name_hash, fallback))
}

pub(in crate::native_engine::css) fn parse_margin_property(
    value: &str,
) -> Option<MarginDeclarationValue> {
    parse_margin_value(value)
        .map(MarginDeclarationValue::Value)
        .or_else(|| parse_margin_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_local_physical_margin_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<MarginDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(
            MarginDeclarationValue::Value(NativeMarginValue::Length(0)),
        ));
    }
    parse_local_cascade_declaration(value, parse_margin_property)
}

pub(in crate::native_engine::css) fn parse_local_margin_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeMarginValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)));
    }
    parse_local_cascade_declaration(value, parse_margin_value)
}

pub(in crate::native_engine::css) fn parse_box_sizing_custom_property(
    value: &str,
) -> Option<BoxSizingDeclarationValue> {
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
        Some(fallback) => parse_box_sizing(fallback.trim())
            .map(|fallback| BoxSizingDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(BoxSizingDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_box_sizing_property(
    value: &str,
) -> Option<BoxSizingDeclarationValue> {
    parse_box_sizing(value)
        .map(BoxSizingDeclarationValue::Value)
        .or_else(|| parse_box_sizing_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_local_box_sizing_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<BoxSizingDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::ContentBox),
        ));
    }
    parse_local_cascade_declaration(value, parse_box_sizing_property)
}

pub(in crate::native_engine::css) fn is_local_reset_keyword(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("initial")
        || value.eq_ignore_ascii_case("unset")
        || value.eq_ignore_ascii_case("revert")
}

pub(in crate::native_engine::css) fn is_inherit_keyword(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("inherit")
}

pub(in crate::native_engine::css) fn parse_text_overflow(value: &str) -> Option<TextOverflowValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "clip" => Some(TextOverflowValue::Clip),
        "ellipsis" => Some(TextOverflowValue::Ellipsis),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_vertical_align(
    value: &str,
) -> Option<VerticalAlignValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "baseline" => Some(VerticalAlignValue::Baseline),
        "top" => Some(VerticalAlignValue::Top),
        "middle" => Some(VerticalAlignValue::Middle),
        "bottom" => Some(VerticalAlignValue::Bottom),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_vertical_align_custom_property(
    value: &str,
) -> Option<VerticalAlignDeclarationValue> {
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
        Some(fallback) => parse_vertical_align(fallback).map(|fallback| {
            VerticalAlignDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(VerticalAlignDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_vertical_align_property(
    value: &str,
) -> Option<VerticalAlignDeclarationValue> {
    parse_vertical_align(value)
        .map(VerticalAlignDeclarationValue::Value)
        .or_else(|| parse_vertical_align_custom_property(value))
}
pub(in crate::native_engine::css) fn parse_visibility_var_arguments(
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

pub(in crate::native_engine::css) fn parse_visibility_custom_property(
    value: &str,
) -> Option<VisibilityValue> {
    let (name_hash, fallback) = parse_visibility_var_arguments(value)?;
    match fallback {
        Some(fallback) => match parse_visibility(fallback)? {
            VisibilityValue::Hidden => {
                Some(VisibilityValue::CustomPropertyFallback(name_hash, true))
            }
            VisibilityValue::Other => {
                Some(VisibilityValue::CustomPropertyFallback(name_hash, false))
            }
            VisibilityValue::CustomProperty(_) | VisibilityValue::CustomPropertyFallback(_, _) => {
                None
            }
        },
        None => Some(VisibilityValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_visibility(value: &str) -> Option<VisibilityValue> {
    match value.to_ascii_lowercase().as_str() {
        "hidden" => Some(VisibilityValue::Hidden),
        "visible" => Some(VisibilityValue::Other),
        _ => parse_visibility_custom_property(value),
    }
}

pub(in crate::native_engine::css) fn parse_visibility_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<VisibilityValue>> {
    parse_local_cascade_declaration(value, parse_visibility)
}

pub(in crate::native_engine::css) fn parse_overflow(value: &str) -> Option<OverflowValue> {
    match value.to_ascii_lowercase().as_str() {
        "hidden" => Some(OverflowValue::Hidden),
        "clip" => Some(OverflowValue::Clip),
        "visible" => Some(OverflowValue::Other),
        "auto" => Some(OverflowValue::Auto),
        "scroll" => Some(OverflowValue::Scroll),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_overflow_custom_property(
    value: &str,
) -> Option<OverflowDeclarationValue> {
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
        Some(fallback) => parse_overflow(fallback.trim())
            .map(|fallback| OverflowDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(OverflowDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_overflow_property(
    value: &str,
) -> Option<OverflowDeclarationValue> {
    parse_overflow(value)
        .map(OverflowDeclarationValue::Value)
        .or_else(|| parse_overflow_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_overflow_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<OverflowDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_overflow_property)
}
