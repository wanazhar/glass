use super::*;

pub(in crate::native_engine::css) fn parse_font_size_calculation_term_any(
    value: &str,
) -> Option<NativeFontSizeCalculation> {
    let value = value.trim().to_ascii_lowercase();
    if value.starts_with("calc(") {
        return parse_font_size_calculation(&value);
    }
    if let Some(term) = parse_font_size_calculation_term(&value) {
        return Some(term);
    }
    let (scale_milli, viewport_unit, root_relative) =
        parse_font_size_calculation_relative_term(&value)?;
    let mut term = NativeFontSizeCalculation::default();
    match viewport_unit {
        Some(NativeViewportFontSizeUnit::Width) => {
            term.viewport_width_scale_milli = i64::from(scale_milli)
        }
        Some(NativeViewportFontSizeUnit::Height) => {
            term.viewport_height_scale_milli = i64::from(scale_milli)
        }
        Some(NativeViewportFontSizeUnit::Min) => {
            term.viewport_min_scale_milli = i64::from(scale_milli);
        }
        Some(NativeViewportFontSizeUnit::Max) => {
            term.viewport_max_scale_milli = i64::from(scale_milli);
        }
        None if root_relative => term.root_scale_milli = i64::from(scale_milli),
        None => term.parent_scale_milli = i64::from(scale_milli),
    }
    Some(term)
}

pub(in crate::native_engine::css) fn scale_font_size_calculation(
    calculation: NativeFontSizeCalculation,
    factor_milli: u32,
    divide: bool,
) -> Option<NativeFontSizeCalculation> {
    let factor = i64::from(factor_milli);
    if divide && factor == 0 {
        return None;
    }
    let scale = |value: i64| {
        let denominator = if divide { factor } else { 1_000 };
        let product = if divide {
            value.checked_mul(1_000)?
        } else {
            value.checked_mul(factor)?
        };
        if product >= 0 {
            product
                .checked_add(denominator / 2)?
                .checked_div(denominator)
        } else {
            product
                .checked_sub(denominator / 2)?
                .checked_div(denominator)
        }
    };
    Some(NativeFontSizeCalculation {
        absolute_milli: scale(calculation.absolute_milli)?,
        parent_scale_milli: scale(calculation.parent_scale_milli)?,
        root_scale_milli: scale(calculation.root_scale_milli)?,
        viewport_width_scale_milli: scale(calculation.viewport_width_scale_milli)?,
        viewport_height_scale_milli: scale(calculation.viewport_height_scale_milli)?,
        viewport_min_scale_milli: scale(calculation.viewport_min_scale_milli)?,
        viewport_max_scale_milli: scale(calculation.viewport_max_scale_milli)?,
    })
}

pub(in crate::native_engine::css) fn parse_font_size_calculation_product(
    value: &str,
) -> Option<NativeFontSizeCalculation> {
    enum ProductValue {
        Calculation(NativeFontSizeCalculation),
        Unitless(u32),
    }

    fn parse_operand(value: &str) -> Option<ProductValue> {
        parse_font_size_calculation_term_any(value)
            .map(ProductValue::Calculation)
            .or_else(|| {
                parse_font_size_decimal_milli(value)
                    .filter(|factor| {
                        *factor
                            <= crate::native_engine::font::MAX_NATIVE_FONT_SIZE
                                .saturating_mul(1_000)
                    })
                    .map(ProductValue::Unitless)
            })
    }

    fn scale_unitless(lhs: u32, rhs: u32, divide: bool) -> Option<u32> {
        if divide && rhs == 0 {
            return None;
        }
        let denominator = if divide { u64::from(rhs) } else { 1_000 };
        let product = u64::from(lhs).checked_mul(if divide { 1_000 } else { u64::from(rhs) })?;
        let rounded = product
            .checked_add(denominator / 2)?
            .checked_div(denominator)?;
        u32::try_from(rounded).ok()
    }

    fn apply_product(
        left: ProductValue,
        operator: u8,
        right: ProductValue,
    ) -> Option<ProductValue> {
        let divide = operator == b'/';
        match (left, right, divide) {
            (ProductValue::Calculation(calculation), ProductValue::Unitless(factor), divide) => {
                scale_font_size_calculation(calculation, factor, divide)
                    .map(ProductValue::Calculation)
            }
            (ProductValue::Unitless(factor), ProductValue::Calculation(calculation), false) => {
                scale_font_size_calculation(calculation, factor, false)
                    .map(ProductValue::Calculation)
            }
            (ProductValue::Unitless(left), ProductValue::Unitless(right), divide) => {
                scale_unitless(left, right, divide).map(ProductValue::Unitless)
            }
            _ => None,
        }
    }

    let mut current = None;
    let mut operator = None;
    let mut operand_start = 0;
    let mut operand_count = 0_usize;
    let mut depth = 0_usize;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => depth = depth.checked_add(1)?,
            b')' => {
                if depth == 0 {
                    return None;
                }
                depth = depth.saturating_sub(1);
            }
            b'*' | b'/' if depth == 0 => {
                let operand = value.get(operand_start..index)?.trim();
                if operand.is_empty() {
                    return None;
                }
                operand_count = operand_count.saturating_add(1);
                if operand_count > MAX_NATIVE_FONT_SIZE_CALC_PRODUCT_OPERANDS {
                    return None;
                }
                let parsed = parse_operand(operand)?;
                if let Some(left) = current.take() {
                    current = Some(apply_product(left, operator?, parsed)?);
                } else if operator.is_some() {
                    return None;
                } else {
                    current = Some(parsed);
                }
                operator = Some(byte);
                operand_start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }

    let operand = value.get(operand_start..)?.trim();
    if operand.is_empty() {
        return None;
    }
    operand_count = operand_count.saturating_add(1);
    if operand_count > MAX_NATIVE_FONT_SIZE_CALC_PRODUCT_OPERANDS {
        return None;
    }
    let parsed = parse_operand(operand)?;
    if let Some(left) = current.take() {
        current = Some(apply_product(left, operator?, parsed)?);
    } else if operator.is_some() {
        return None;
    } else {
        current = Some(parsed);
    }

    match current? {
        ProductValue::Calculation(calculation) => Some(calculation),
        ProductValue::Unitless(_) => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_size_calculation(
    value: &str,
) -> Option<NativeFontSizeCalculation> {
    let value = value.trim().to_ascii_lowercase();
    let source = value.strip_prefix("calc(")?.strip_suffix(')')?;
    if source.trim().is_empty() {
        return None;
    }
    let mut total = NativeFontSizeCalculation::default();
    let mut start = 0;
    let mut sign = 1_i64;
    let mut term_count: usize = 0;
    let mut depth = 0_usize;
    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'(' => depth = depth.checked_add(1)?,
            b')' => {
                if depth == 0 {
                    return None;
                }
                depth = depth.saturating_sub(1);
            }
            b'+' | b'-' if depth == 0 => {
                let term = source.get(start..index)?.trim();
                if term.is_empty() {
                    return None;
                }
                term_count = term_count.saturating_add(1);
                if term_count > MAX_NATIVE_FONT_SIZE_CALC_TERMS {
                    return None;
                }
                merge_font_size_calculation(
                    &mut total,
                    parse_font_size_calculation_product(term)?,
                    sign,
                )?;
                sign = if byte == b'+' { 1 } else { -1 };
                start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let term = source.get(start..)?.trim();
    if term.is_empty() {
        return None;
    }
    term_count = term_count.saturating_add(1);
    if term_count > MAX_NATIVE_FONT_SIZE_CALC_TERMS {
        return None;
    }
    merge_font_size_calculation(&mut total, parse_font_size_calculation_product(term)?, sign)?;
    Some(total)
}

pub(in crate::native_engine::css) fn parse_font_size_variable_expression(
    value: &str,
) -> Option<NativeFontSizeVariableExpression> {
    let value = value.trim();
    if value.len() < 9
        || !value
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("calc("))
        || !value.ends_with(')')
        || !value.is_ascii()
    {
        return None;
    }
    let bytes = value.as_bytes();
    let mut depth = 0_usize;
    let mut has_variable = false;
    for index in 0..bytes.len() {
        if bytes
            .get(index..index.saturating_add(4))
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(b"var("))
        {
            has_variable = true;
        }
        match bytes[index] {
            b'(' => depth = depth.checked_add(1)?,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && index.saturating_add(1) != bytes.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    (depth == 0 && has_variable).then(|| NativeFontSizeVariableExpression::from_text(value))?
}

pub(in crate::native_engine::css) fn parse_font_size_value_without_custom(
    value: &str,
) -> Option<NativeFontSizeDeclarationValue> {
    parse_font_size_variable_expression(value)
        .map(NativeFontSizeDeclarationValue::VariableCalculation)
        .or_else(|| parse_font_size(value).map(NativeFontSizeDeclarationValue::Pixels))
        .or_else(|| parse_relative_font_size(value))
        .or_else(|| {
            parse_font_size_calculation(value).map(NativeFontSizeDeclarationValue::Calculation)
        })
}

pub(in crate::native_engine::css) fn parse_font_size_value(
    value: &str,
) -> Option<NativeFontSizeDeclarationValue> {
    parse_font_size_value_without_custom(value).or_else(|| parse_font_size_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_word_break(value: &str) -> Option<WordBreakValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(WordBreakValue::Normal),
        "break-all" => Some(WordBreakValue::BreakAll),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_word_break_custom_property(
    value: &str,
) -> Option<WordBreakDeclarationValue> {
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
        Some(fallback) => parse_word_break(fallback)
            .map(|fallback| WordBreakDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(WordBreakDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_word_break_property(
    value: &str,
) -> Option<WordBreakDeclarationValue> {
    parse_word_break(value)
        .map(WordBreakDeclarationValue::Value)
        .or_else(|| parse_word_break_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_inherited_text_declaration<T: Copy>(
    value: &str,
    parse: fn(&str) -> Option<T>,
) -> Option<InheritedTextDeclaration<T>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("inherit") {
        return Some(InheritedTextDeclaration::Inherit);
    }
    if value.eq_ignore_ascii_case("initial") {
        return Some(InheritedTextDeclaration::Initial);
    }
    if value.eq_ignore_ascii_case("unset") {
        return Some(InheritedTextDeclaration::Unset);
    }
    if value.eq_ignore_ascii_case("revert") {
        return Some(InheritedTextDeclaration::Revert);
    }
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(InheritedTextDeclaration::RevertLayer);
    }
    parse(value).map(InheritedTextDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_local_cascade_declaration<T: Copy>(
    value: &str,
    parse: fn(&str) -> Option<T>,
) -> Option<LocalCascadeDeclaration<T>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    parse(value).map(LocalCascadeDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_local_reset_cascade_declaration<T: Copy>(
    value: &str,
    parse: fn(&str) -> Option<T>,
) -> Option<LocalCascadeDeclaration<T>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Reset);
    }
    parse(value).map(LocalCascadeDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_text_transform_custom_property(
    value: &str,
) -> Option<TextTransformDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_text_transform(fallback).map(|fallback| {
            TextTransformDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(TextTransformDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_text_transform_property(
    value: &str,
) -> Option<TextTransformDeclarationValue> {
    parse_text_transform(value)
        .map(TextTransformDeclarationValue::Value)
        .or_else(|| parse_text_transform_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_text_transform_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextTransformDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_text_transform_property)
}

pub(in crate::native_engine::css) fn parse_font_variant_ligatures(
    value: &str,
) -> Option<NativeFontVariantLigatures> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.len() == 1 {
        match tokens[0].to_ascii_lowercase().as_str() {
            "normal" => return Some(NativeFontVariantLigatures::default()),
            "none" => {
                return Some(NativeFontVariantLigatures {
                    common: false,
                    discretionary: false,
                    historical: false,
                    contextual: false,
                });
            }
            _ => {}
        }
    }
    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| matches!(token.to_ascii_lowercase().as_str(), "normal" | "none"))
    {
        return None;
    }
    let mut result = NativeFontVariantLigatures::default();
    let mut groups = [false; 4];
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "common-ligatures" if !groups[0] => {
                groups[0] = true;
                result.common = true;
            }
            "no-common-ligatures" if !groups[0] => {
                groups[0] = true;
                result.common = false;
            }
            "discretionary-ligatures" if !groups[1] => {
                groups[1] = true;
                result.discretionary = true;
            }
            "no-discretionary-ligatures" if !groups[1] => {
                groups[1] = true;
                result.discretionary = false;
            }
            "historical-ligatures" if !groups[2] => {
                groups[2] = true;
                result.historical = true;
            }
            "no-historical-ligatures" if !groups[2] => {
                groups[2] = true;
                result.historical = false;
            }
            "contextual" if !groups[3] => {
                groups[3] = true;
                result.contextual = true;
            }
            "no-contextual" if !groups[3] => {
                groups[3] = true;
                result.contextual = false;
            }
            _ => return None,
        }
    }
    Some(result)
}

pub(in crate::native_engine::css) fn parse_font_variant_ligatures_custom_property(
    value: &str,
) -> Option<FontVariantLigaturesDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variant_ligatures(fallback).map(|fallback| {
            FontVariantLigaturesDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariantLigaturesDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_ligatures_property(
    value: &str,
) -> Option<FontVariantLigaturesDeclarationValue> {
    parse_font_variant_ligatures(value)
        .map(FontVariantLigaturesDeclarationValue::Value)
        .or_else(|| parse_font_variant_ligatures_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variant_ligatures_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantLigaturesDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variant_ligatures_property)
}

pub(in crate::native_engine::css) fn wrap_font_variant_ligatures_declaration(
    declaration: InheritedTextDeclaration<NativeFontVariantLigatures>,
) -> InheritedTextDeclaration<FontVariantLigaturesDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            InheritedTextDeclaration::Value(FontVariantLigaturesDeclarationValue::Value(value))
        }
        InheritedTextDeclaration::Inherit => InheritedTextDeclaration::Inherit,
        InheritedTextDeclaration::Initial => InheritedTextDeclaration::Initial,
        InheritedTextDeclaration::Unset => InheritedTextDeclaration::Unset,
        InheritedTextDeclaration::Revert => InheritedTextDeclaration::Revert,
        InheritedTextDeclaration::RevertLayer => InheritedTextDeclaration::RevertLayer,
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_caps(
    value: &str,
) -> Option<NativeFontVariantCaps> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(NativeFontVariantCaps::Normal),
        "small-caps" => Some(NativeFontVariantCaps::SmallCaps),
        "all-small-caps" => Some(NativeFontVariantCaps::AllSmallCaps),
        "petite-caps" => Some(NativeFontVariantCaps::PetiteCaps),
        "all-petite-caps" => Some(NativeFontVariantCaps::AllPetiteCaps),
        "unicase" => Some(NativeFontVariantCaps::Unicase),
        "titling-caps" => Some(NativeFontVariantCaps::TitlingCaps),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_caps_custom_property(
    value: &str,
) -> Option<FontVariantCapsDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variant_caps(fallback).map(|fallback| {
            FontVariantCapsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariantCapsDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_caps_property(
    value: &str,
) -> Option<FontVariantCapsDeclarationValue> {
    parse_font_variant_caps(value)
        .map(FontVariantCapsDeclarationValue::Value)
        .or_else(|| parse_font_variant_caps_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variant_caps_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantCapsDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variant_caps_property)
}

pub(in crate::native_engine::css) fn wrap_font_variant_caps_declaration(
    declaration: InheritedTextDeclaration<NativeFontVariantCaps>,
) -> InheritedTextDeclaration<FontVariantCapsDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            InheritedTextDeclaration::Value(FontVariantCapsDeclarationValue::Value(value))
        }
        InheritedTextDeclaration::Inherit => InheritedTextDeclaration::Inherit,
        InheritedTextDeclaration::Initial => InheritedTextDeclaration::Initial,
        InheritedTextDeclaration::Unset => InheritedTextDeclaration::Unset,
        InheritedTextDeclaration::Revert => InheritedTextDeclaration::Revert,
        InheritedTextDeclaration::RevertLayer => InheritedTextDeclaration::RevertLayer,
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_position(
    value: &str,
) -> Option<NativeFontVariantPosition> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(NativeFontVariantPosition::Normal),
        "sub" => Some(NativeFontVariantPosition::Sub),
        "super" => Some(NativeFontVariantPosition::Super),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_position_custom_property(
    value: &str,
) -> Option<FontVariantPositionDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variant_position(fallback).map(|fallback| {
            FontVariantPositionDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariantPositionDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_position_property(
    value: &str,
) -> Option<FontVariantPositionDeclarationValue> {
    parse_font_variant_position(value)
        .map(FontVariantPositionDeclarationValue::Value)
        .or_else(|| parse_font_variant_position_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variant_position_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantPositionDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variant_position_property)
}

pub(in crate::native_engine::css) fn wrap_font_variant_position_declaration(
    declaration: InheritedTextDeclaration<NativeFontVariantPosition>,
) -> InheritedTextDeclaration<FontVariantPositionDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            InheritedTextDeclaration::Value(FontVariantPositionDeclarationValue::Value(value))
        }
        InheritedTextDeclaration::Inherit => InheritedTextDeclaration::Inherit,
        InheritedTextDeclaration::Initial => InheritedTextDeclaration::Initial,
        InheritedTextDeclaration::Unset => InheritedTextDeclaration::Unset,
        InheritedTextDeclaration::Revert => InheritedTextDeclaration::Revert,
        InheritedTextDeclaration::RevertLayer => InheritedTextDeclaration::RevertLayer,
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_alternates(
    value: &str,
) -> Option<NativeFontVariantAlternates> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(NativeFontVariantAlternates::Normal),
        "historical-forms" => Some(NativeFontVariantAlternates::HistoricalForms),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_alternates_custom_property(
    value: &str,
) -> Option<FontVariantAlternatesDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variant_alternates(fallback).map(|fallback| {
            FontVariantAlternatesDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariantAlternatesDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_alternates_property(
    value: &str,
) -> Option<FontVariantAlternatesDeclarationValue> {
    parse_font_variant_alternates(value)
        .map(FontVariantAlternatesDeclarationValue::Value)
        .or_else(|| parse_font_variant_alternates_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variant_alternates_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantAlternatesDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variant_alternates_property)
}

pub(in crate::native_engine::css) fn wrap_font_variant_alternates_declaration(
    declaration: InheritedTextDeclaration<NativeFontVariantAlternates>,
) -> InheritedTextDeclaration<FontVariantAlternatesDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            InheritedTextDeclaration::Value(FontVariantAlternatesDeclarationValue::Value(value))
        }
        InheritedTextDeclaration::Inherit => InheritedTextDeclaration::Inherit,
        InheritedTextDeclaration::Initial => InheritedTextDeclaration::Initial,
        InheritedTextDeclaration::Unset => InheritedTextDeclaration::Unset,
        InheritedTextDeclaration::Revert => InheritedTextDeclaration::Revert,
        InheritedTextDeclaration::RevertLayer => InheritedTextDeclaration::RevertLayer,
    }
}

pub(in crate::native_engine::css) fn parse_font_language_override(
    value: &str,
) -> Option<NativeFontLanguageOverride> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("normal") {
        return Some(NativeFontLanguageOverride::default());
    }
    let bytes = value.as_bytes();
    if bytes.len() != 6 || !matches!(bytes[0], b'\'' | b'"') || bytes[5] != bytes[0] {
        return None;
    }
    let tag: [u8; 4] = bytes[1..5].try_into().ok()?;
    if tag
        .iter()
        .any(|byte| !(*byte >= 0x20 && *byte <= 0x7e) || matches!(*byte, b'\'' | b'"' | b'\\'))
    {
        return None;
    }
    let language_end = tag
        .iter()
        .position(|byte| *byte == b' ')
        .unwrap_or(tag.len());
    if language_end == 0 || !tag[language_end..].iter().all(|byte| *byte == b' ') {
        return None;
    }
    Some(NativeFontLanguageOverride { tag: Some(tag) })
}

pub(in crate::native_engine::css) fn parse_font_language_override_custom_property(
    value: &str,
) -> Option<FontLanguageOverrideDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_language_override(fallback).map(|fallback| {
            FontLanguageOverrideDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontLanguageOverrideDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_language_override_property(
    value: &str,
) -> Option<FontLanguageOverrideDeclarationValue> {
    parse_font_language_override(value)
        .map(FontLanguageOverrideDeclarationValue::Value)
        .or_else(|| parse_font_language_override_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_language_override_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontLanguageOverrideDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_language_override_property)
}

pub(in crate::native_engine::css) fn parse_signed_decimal_milli(value: &str) -> Option<i32> {
    let value = value.trim();
    let (negative, unsigned) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else if let Some(value) = value.strip_prefix('+') {
        (false, value)
    } else {
        (false, value)
    };
    let magnitude = i64::from(parse_decimal_milli(unsigned)?);
    if magnitude > i64::from(MAX_NATIVE_FONT_VARIATION_VALUE_MILLI) {
        return None;
    }
    let signed = if negative { -magnitude } else { magnitude };
    i32::try_from(signed).ok()
}

pub(in crate::native_engine::css) fn parse_font_variation_entry(
    value: &str,
) -> Option<NativeFontVariation> {
    let bytes = value.trim().as_bytes();
    if bytes.len() < 8 {
        return None;
    }
    let quote = *bytes.first()?;
    if !matches!(quote, b'\'' | b'"') || bytes[5] != quote {
        return None;
    }
    let tag: [u8; 4] = bytes[1..5].try_into().ok()?;
    if tag.iter().any(|byte| {
        !(*byte >= 0x20 && *byte <= 0x7e) || matches!(*byte, b'\'' | b'"' | b'\\' | b',')
    }) {
        return None;
    }
    let suffix = std::str::from_utf8(&bytes[6..]).ok()?.trim();
    if suffix.is_empty() || suffix.split_ascii_whitespace().count() != 1 {
        return None;
    }
    Some(NativeFontVariation {
        tag,
        value_milli: parse_signed_decimal_milli(suffix)?,
    })
}

pub(in crate::native_engine::css) fn parse_font_variation_settings_custom_property(
    value: &str,
) -> Option<FontVariationSettingsDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_family_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variation_settings(fallback).map(|fallback| {
            FontVariationSettingsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariationSettingsDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_variation_settings_property(
    value: &str,
) -> Option<FontVariationSettingsDeclarationValue> {
    parse_font_variation_settings(value)
        .map(FontVariationSettingsDeclarationValue::Value)
        .or_else(|| parse_font_variation_settings_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variation_settings_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariationSettingsDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variation_settings_property)
}

pub(in crate::native_engine::css) fn parse_font_variant_east_asian(
    value: &str,
) -> Option<NativeFontVariantEastAsian> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("normal") {
        return Some(NativeFontVariantEastAsian::default());
    }
    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("normal"))
    {
        return None;
    }

    let mut result = NativeFontVariantEastAsian::default();
    let mut form_seen = false;
    let mut width_seen = false;
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "jis78" if !form_seen => {
                form_seen = true;
                result.form = NativeFontVariantEastAsianForm::Jis78;
            }
            "jis83" if !form_seen => {
                form_seen = true;
                result.form = NativeFontVariantEastAsianForm::Jis83;
            }
            "jis90" if !form_seen => {
                form_seen = true;
                result.form = NativeFontVariantEastAsianForm::Jis90;
            }
            "jis04" if !form_seen => {
                form_seen = true;
                result.form = NativeFontVariantEastAsianForm::Jis04;
            }
            "simplified" if !form_seen => {
                form_seen = true;
                result.form = NativeFontVariantEastAsianForm::Simplified;
            }
            "traditional" if !form_seen => {
                form_seen = true;
                result.form = NativeFontVariantEastAsianForm::Traditional;
            }
            "full-width" if !width_seen => {
                width_seen = true;
                result.width = NativeFontVariantEastAsianWidth::Full;
            }
            "proportional-width" if !width_seen => {
                width_seen = true;
                result.width = NativeFontVariantEastAsianWidth::Proportional;
            }
            "ruby" if !result.ruby => result.ruby = true,
            _ => return None,
        }
    }
    Some(result)
}

pub(in crate::native_engine::css) fn parse_font_variant_east_asian_custom_property(
    value: &str,
) -> Option<FontVariantEastAsianDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variant_east_asian(fallback).map(|fallback| {
            FontVariantEastAsianDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariantEastAsianDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_east_asian_property(
    value: &str,
) -> Option<FontVariantEastAsianDeclarationValue> {
    parse_font_variant_east_asian(value)
        .map(FontVariantEastAsianDeclarationValue::Value)
        .or_else(|| parse_font_variant_east_asian_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variant_east_asian_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantEastAsianDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variant_east_asian_property)
}

pub(in crate::native_engine::css) fn wrap_font_variant_east_asian_declaration(
    declaration: InheritedTextDeclaration<NativeFontVariantEastAsian>,
) -> InheritedTextDeclaration<FontVariantEastAsianDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            InheritedTextDeclaration::Value(FontVariantEastAsianDeclarationValue::Value(value))
        }
        InheritedTextDeclaration::Inherit => InheritedTextDeclaration::Inherit,
        InheritedTextDeclaration::Initial => InheritedTextDeclaration::Initial,
        InheritedTextDeclaration::Unset => InheritedTextDeclaration::Unset,
        InheritedTextDeclaration::Revert => InheritedTextDeclaration::Revert,
        InheritedTextDeclaration::RevertLayer => InheritedTextDeclaration::RevertLayer,
    }
}
pub(in crate::native_engine::css) fn parse_font_variant_shorthand_custom_property(
    value: &str,
) -> Option<FontVariantShorthandCustomPropertyDeclaration> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    let fallback = match fallback {
        Some(fallback) => Some(parse_font_variant_shorthand(fallback)?),
        None => None,
    };
    match fallback {
        None => Some(FontVariantShorthandCustomPropertyDeclaration {
            ligatures: InheritedTextDeclaration::Value(
                FontVariantLigaturesDeclarationValue::CustomProperty(name_hash),
            ),
            caps: InheritedTextDeclaration::Value(FontVariantCapsDeclarationValue::CustomProperty(
                name_hash,
            )),
            position: InheritedTextDeclaration::Value(
                FontVariantPositionDeclarationValue::CustomProperty(name_hash),
            ),
            alternates: InheritedTextDeclaration::Value(
                FontVariantAlternatesDeclarationValue::CustomProperty(name_hash),
            ),
            east_asian: InheritedTextDeclaration::Value(
                FontVariantEastAsianDeclarationValue::CustomProperty(name_hash),
            ),
            numeric: InheritedTextDeclaration::Value(
                FontVariantNumericDeclarationValue::CustomProperty(name_hash),
            ),
        }),
        Some(fallback) => {
            let InheritedTextDeclaration::Value(ligatures) = fallback.ligatures else {
                return None;
            };
            let InheritedTextDeclaration::Value(caps) = fallback.caps else {
                return None;
            };
            let InheritedTextDeclaration::Value(position) = fallback.position else {
                return None;
            };
            let InheritedTextDeclaration::Value(alternates) = fallback.alternates else {
                return None;
            };
            let InheritedTextDeclaration::Value(east_asian) = fallback.east_asian else {
                return None;
            };
            let InheritedTextDeclaration::Value(numeric) = fallback.numeric else {
                return None;
            };
            Some(FontVariantShorthandCustomPropertyDeclaration {
                ligatures: InheritedTextDeclaration::Value(
                    FontVariantLigaturesDeclarationValue::CustomPropertyFallback(
                        name_hash, ligatures,
                    ),
                ),
                caps: InheritedTextDeclaration::Value(
                    FontVariantCapsDeclarationValue::CustomPropertyFallback(name_hash, caps),
                ),
                position: InheritedTextDeclaration::Value(
                    FontVariantPositionDeclarationValue::CustomPropertyFallback(
                        name_hash, position,
                    ),
                ),
                alternates: InheritedTextDeclaration::Value(
                    FontVariantAlternatesDeclarationValue::CustomPropertyFallback(
                        name_hash, alternates,
                    ),
                ),
                east_asian: InheritedTextDeclaration::Value(
                    FontVariantEastAsianDeclarationValue::CustomPropertyFallback(
                        name_hash, east_asian,
                    ),
                ),
                numeric: InheritedTextDeclaration::Value(
                    FontVariantNumericDeclarationValue::CustomPropertyFallback(name_hash, numeric),
                ),
            })
        }
    }
}
pub(in crate::native_engine::css) fn parse_font_variant_ligatures_custom_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantLigaturesDeclarationValue>> {
    parse_font_variant_ligatures_declaration(value).or_else(|| {
        parse_font_variant_shorthand(value)
            .map(|parsed| wrap_font_variant_ligatures_declaration(parsed.ligatures))
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_caps_custom_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantCapsDeclarationValue>> {
    parse_font_variant_caps_declaration(value).or_else(|| {
        parse_font_variant_shorthand(value)
            .map(|parsed| wrap_font_variant_caps_declaration(parsed.caps))
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_position_custom_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantPositionDeclarationValue>> {
    parse_font_variant_position_declaration(value).or_else(|| {
        parse_font_variant_shorthand(value)
            .map(|parsed| wrap_font_variant_position_declaration(parsed.position))
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_alternates_custom_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantAlternatesDeclarationValue>> {
    parse_font_variant_alternates_declaration(value).or_else(|| {
        parse_font_variant_shorthand(value)
            .map(|parsed| wrap_font_variant_alternates_declaration(parsed.alternates))
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_east_asian_custom_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantEastAsianDeclarationValue>> {
    parse_font_variant_east_asian_declaration(value).or_else(|| {
        parse_font_variant_shorthand(value)
            .map(|parsed| wrap_font_variant_east_asian_declaration(parsed.east_asian))
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_numeric_custom_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantNumericDeclarationValue>> {
    parse_font_variant_numeric_declaration(value).or_else(|| {
        parse_font_variant_shorthand(value)
            .map(|parsed| wrap_font_variant_numeric_declaration(parsed.numeric))
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_shorthand(
    value: &str,
) -> Option<NativeFontVariantShorthand> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }
    if tokens.len() == 1 {
        match tokens[0].to_ascii_lowercase().as_str() {
            "inherit" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::Inherit,
                    caps: InheritedTextDeclaration::Inherit,
                    position: InheritedTextDeclaration::Inherit,
                    alternates: InheritedTextDeclaration::Inherit,
                    east_asian: InheritedTextDeclaration::Inherit,
                    numeric: InheritedTextDeclaration::Inherit,
                });
            }
            "initial" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::Initial,
                    caps: InheritedTextDeclaration::Initial,
                    position: InheritedTextDeclaration::Initial,
                    alternates: InheritedTextDeclaration::Initial,
                    east_asian: InheritedTextDeclaration::Initial,
                    numeric: InheritedTextDeclaration::Initial,
                });
            }
            "unset" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::Unset,
                    caps: InheritedTextDeclaration::Unset,
                    position: InheritedTextDeclaration::Unset,
                    alternates: InheritedTextDeclaration::Unset,
                    east_asian: InheritedTextDeclaration::Unset,
                    numeric: InheritedTextDeclaration::Unset,
                });
            }
            "revert" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::Revert,
                    caps: InheritedTextDeclaration::Revert,
                    position: InheritedTextDeclaration::Revert,
                    alternates: InheritedTextDeclaration::Revert,
                    east_asian: InheritedTextDeclaration::Revert,
                    numeric: InheritedTextDeclaration::Revert,
                });
            }
            "revert-layer" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::RevertLayer,
                    caps: InheritedTextDeclaration::RevertLayer,
                    position: InheritedTextDeclaration::RevertLayer,
                    alternates: InheritedTextDeclaration::RevertLayer,
                    east_asian: InheritedTextDeclaration::RevertLayer,
                    numeric: InheritedTextDeclaration::RevertLayer,
                });
            }
            "normal" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::Value(
                        NativeFontVariantLigatures::default(),
                    ),
                    caps: InheritedTextDeclaration::Value(NativeFontVariantCaps::Normal),
                    position: InheritedTextDeclaration::Value(NativeFontVariantPosition::Normal),
                    alternates: InheritedTextDeclaration::Value(
                        NativeFontVariantAlternates::Normal,
                    ),
                    east_asian: InheritedTextDeclaration::Value(
                        NativeFontVariantEastAsian::default(),
                    ),
                    numeric: InheritedTextDeclaration::Value(NativeFontVariantNumeric::default()),
                });
            }
            "none" => {
                return Some(NativeFontVariantShorthand {
                    ligatures: InheritedTextDeclaration::Value(NativeFontVariantLigatures {
                        common: false,
                        discretionary: false,
                        historical: false,
                        contextual: false,
                    }),
                    caps: InheritedTextDeclaration::Value(NativeFontVariantCaps::Normal),
                    position: InheritedTextDeclaration::Value(NativeFontVariantPosition::Normal),
                    alternates: InheritedTextDeclaration::Value(
                        NativeFontVariantAlternates::Normal,
                    ),
                    east_asian: InheritedTextDeclaration::Value(
                        NativeFontVariantEastAsian::default(),
                    ),
                    numeric: InheritedTextDeclaration::Value(NativeFontVariantNumeric::default()),
                });
            }
            _ => {}
        }
    }
    if tokens.iter().any(|token| {
        matches!(
            token.to_ascii_lowercase().as_str(),
            "normal" | "none" | "inherit" | "initial" | "unset" | "revert" | "revert-layer"
        )
    }) {
        return None;
    }

    let mut ligature_tokens = Vec::new();
    let mut caps_token = None;
    let mut position_token = None;
    let mut alternate_token = None;
    let mut east_asian_tokens = Vec::new();
    let mut numeric_tokens = Vec::new();
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "common-ligatures"
            | "no-common-ligatures"
            | "discretionary-ligatures"
            | "no-discretionary-ligatures"
            | "historical-ligatures"
            | "no-historical-ligatures"
            | "contextual"
            | "no-contextual" => ligature_tokens.push(token),
            "small-caps" | "all-small-caps" | "petite-caps" | "all-petite-caps" | "unicase"
            | "titling-caps" => {
                if caps_token.is_some() {
                    return None;
                }
                caps_token = Some(token);
            }
            "sub" | "super" => {
                if position_token.is_some() {
                    return None;
                }
                position_token = Some(token);
            }
            "historical-forms" => {
                if alternate_token.is_some() {
                    return None;
                }
                alternate_token = Some(token);
            }
            "jis78" | "jis83" | "jis90" | "jis04" | "simplified" | "traditional" | "full-width"
            | "proportional-width" | "ruby" => east_asian_tokens.push(token),
            "lining-nums" | "oldstyle-nums" | "proportional-nums" | "tabular-nums"
            | "diagonal-fractions" | "stacked-fractions" | "ordinal" | "slashed-zero" => {
                numeric_tokens.push(token)
            }
            _ => return None,
        }
    }
    let ligatures = if ligature_tokens.is_empty() {
        NativeFontVariantLigatures::default()
    } else {
        parse_font_variant_ligatures(&ligature_tokens.join(" "))?
    };
    let caps = caps_token.map_or(Some(NativeFontVariantCaps::Normal), parse_font_variant_caps)?;
    let position = position_token.map_or(
        Some(NativeFontVariantPosition::Normal),
        parse_font_variant_position,
    )?;
    let alternates = alternate_token.map_or(
        Some(NativeFontVariantAlternates::Normal),
        parse_font_variant_alternates,
    )?;
    let east_asian = if east_asian_tokens.is_empty() {
        NativeFontVariantEastAsian::default()
    } else {
        parse_font_variant_east_asian(&east_asian_tokens.join(" "))?
    };
    let numeric = if numeric_tokens.is_empty() {
        NativeFontVariantNumeric::default()
    } else {
        parse_font_variant_numeric(&numeric_tokens.join(" "))?
    };
    Some(NativeFontVariantShorthand {
        ligatures: InheritedTextDeclaration::Value(ligatures),
        caps: InheritedTextDeclaration::Value(caps),
        position: InheritedTextDeclaration::Value(position),
        alternates: InheritedTextDeclaration::Value(alternates),
        east_asian: InheritedTextDeclaration::Value(east_asian),
        numeric: InheritedTextDeclaration::Value(numeric),
    })
}

pub(in crate::native_engine::css) fn parse_font_variant_shorthand_declaration(
    value: &str,
) -> Option<NativeFontVariantShorthand> {
    parse_font_variant_shorthand(value)
}
pub(in crate::native_engine::css) fn parse_font_variant_numeric(
    value: &str,
) -> Option<NativeFontVariantNumeric> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("normal") {
        return Some(NativeFontVariantNumeric::default());
    }
    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("normal"))
    {
        return None;
    }

    let mut result = NativeFontVariantNumeric::default();
    let mut groups = [false; 3];
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "lining-nums" if !groups[0] => {
                groups[0] = true;
                result.figure = NativeFontVariantNumericFigure::Lining;
            }
            "oldstyle-nums" if !groups[0] => {
                groups[0] = true;
                result.figure = NativeFontVariantNumericFigure::Oldstyle;
            }
            "proportional-nums" if !groups[1] => {
                groups[1] = true;
                result.spacing = NativeFontVariantNumericSpacing::Proportional;
            }
            "tabular-nums" if !groups[1] => {
                groups[1] = true;
                result.spacing = NativeFontVariantNumericSpacing::Tabular;
            }
            "diagonal-fractions" if !groups[2] => {
                groups[2] = true;
                result.fraction = NativeFontVariantNumericFraction::Diagonal;
            }
            "stacked-fractions" if !groups[2] => {
                groups[2] = true;
                result.fraction = NativeFontVariantNumericFraction::Stacked;
            }
            "ordinal" if !result.ordinal => result.ordinal = true,
            "slashed-zero" if !result.slashed_zero => result.slashed_zero = true,
            _ => return None,
        }
    }
    Some(result)
}

pub(in crate::native_engine::css) fn parse_font_variant_numeric_custom_property(
    value: &str,
) -> Option<FontVariantNumericDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_variant_numeric(fallback).map(|fallback| {
            FontVariantNumericDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontVariantNumericDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_variant_numeric_property(
    value: &str,
) -> Option<FontVariantNumericDeclarationValue> {
    parse_font_variant_numeric(value)
        .map(FontVariantNumericDeclarationValue::Value)
        .or_else(|| parse_font_variant_numeric_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_variant_numeric_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontVariantNumericDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_variant_numeric_property)
}

pub(in crate::native_engine::css) fn wrap_font_variant_numeric_declaration(
    declaration: InheritedTextDeclaration<NativeFontVariantNumeric>,
) -> InheritedTextDeclaration<FontVariantNumericDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            InheritedTextDeclaration::Value(FontVariantNumericDeclarationValue::Value(value))
        }
        InheritedTextDeclaration::Inherit => InheritedTextDeclaration::Inherit,
        InheritedTextDeclaration::Initial => InheritedTextDeclaration::Initial,
        InheritedTextDeclaration::Unset => InheritedTextDeclaration::Unset,
        InheritedTextDeclaration::Revert => InheritedTextDeclaration::Revert,
        InheritedTextDeclaration::RevertLayer => InheritedTextDeclaration::RevertLayer,
    }
}

pub(in crate::native_engine::css) fn parse_font_feature_entry(
    value: &str,
) -> Option<NativeFontFeature> {
    let bytes = value.trim().as_bytes();
    if bytes.len() < 6 {
        return None;
    }
    let quote = *bytes.first()?;
    if !matches!(quote, b'\'' | b'"') || bytes[5] != quote {
        return None;
    }
    let tag: [u8; 4] = bytes[1..5].try_into().ok()?;
    if tag
        .iter()
        .any(|byte| !(*byte >= 0x20 && *byte <= 0x7e) || matches!(*byte, b'\'' | b'"' | b','))
    {
        return None;
    }
    let suffix = std::str::from_utf8(&bytes[6..]).ok()?.trim();
    let feature_value = if suffix.is_empty() {
        1
    } else {
        let mut tokens = suffix.split_ascii_whitespace();
        let token = tokens.next()?;
        if tokens.next().is_some() {
            return None;
        }
        if token.eq_ignore_ascii_case("on") {
            1
        } else if token.eq_ignore_ascii_case("off") {
            0
        } else {
            let parsed = token.parse::<u32>().ok()?;
            (parsed <= u32::from(u16::MAX)).then_some(parsed)?
        }
    };
    Some(NativeFontFeature {
        tag,
        value: feature_value,
    })
}

pub(in crate::native_engine::css) fn parse_font_feature_settings_custom_property(
    value: &str,
) -> Option<FontFeatureSettingsDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_family_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_feature_settings(fallback).map(|fallback| {
            FontFeatureSettingsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontFeatureSettingsDeclarationValue::CustomProperty(
            name_hash,
        )),
    }
}

pub(in crate::native_engine::css) fn parse_font_feature_settings_property(
    value: &str,
) -> Option<FontFeatureSettingsDeclarationValue> {
    parse_font_feature_settings(value)
        .map(FontFeatureSettingsDeclarationValue::Value)
        .or_else(|| parse_font_feature_settings_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_feature_settings_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontFeatureSettingsDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_feature_settings_property)
}

pub(in crate::native_engine::css) fn parse_font_kerning(value: &str) -> Option<NativeFontKerning> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(NativeFontKerning::Auto),
        "normal" => Some(NativeFontKerning::Normal),
        "none" => Some(NativeFontKerning::None),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_kerning_custom_property(
    value: &str,
) -> Option<FontKerningDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_kerning(fallback).map(|fallback| {
            FontKerningDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontKerningDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_kerning_property(
    value: &str,
) -> Option<FontKerningDeclarationValue> {
    parse_font_kerning(value)
        .map(FontKerningDeclarationValue::Value)
        .or_else(|| parse_font_kerning_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_kerning_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontKerningDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_kerning_property)
}

pub(in crate::native_engine::css) fn parse_font_optical_sizing(
    value: &str,
) -> Option<NativeFontOpticalSizing> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(NativeFontOpticalSizing::Auto),
        "none" => Some(NativeFontOpticalSizing::None),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_font_optical_sizing_custom_property(
    value: &str,
) -> Option<FontOpticalSizingDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_optical_sizing(fallback).map(|fallback| {
            FontOpticalSizingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontOpticalSizingDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_optical_sizing_property(
    value: &str,
) -> Option<FontOpticalSizingDeclarationValue> {
    parse_font_optical_sizing(value)
        .map(FontOpticalSizingDeclarationValue::Value)
        .or_else(|| parse_font_optical_sizing_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_optical_sizing_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontOpticalSizingDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_optical_sizing_property)
}

pub(in crate::native_engine::css) fn parse_font_palette(value: &str) -> Option<NativeFontPalette> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "normal" => Some(NativeFontPalette::Normal),
        "light" => Some(NativeFontPalette::Light),
        "dark" => Some(NativeFontPalette::Dark),
        _ => NativeFontPaletteName::parse(value).map(NativeFontPalette::Named),
    }
}

pub(in crate::native_engine::css) fn parse_font_palette_custom_property(
    value: &str,
) -> Option<FontPaletteDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_palette(fallback).map(|fallback| {
            FontPaletteDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontPaletteDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_palette_property(
    value: &str,
) -> Option<FontPaletteDeclarationValue> {
    parse_font_palette(value)
        .map(FontPaletteDeclarationValue::Value)
        .or_else(|| parse_font_palette_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_palette_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontPaletteDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_palette_property)
}

pub(in crate::native_engine::css) fn parse_font_weight_custom_property(
    value: &str,
) -> Option<FontWeightDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_weight(fallback).map(|fallback| {
            FontWeightDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontWeightDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_weight_property(
    value: &str,
) -> Option<FontWeightDeclarationValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "lighter" => Some(FontWeightDeclarationValue::Lighter),
        "bolder" => Some(FontWeightDeclarationValue::Bolder),
        _ => parse_font_weight(value)
            .map(FontWeightDeclarationValue::Absolute)
            .or_else(|| parse_font_weight_custom_property(value)),
    }
}

pub(in crate::native_engine::css) fn parse_font_weight_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontWeightDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_weight_property)
}

pub(in crate::native_engine::css) fn parse_font_style_custom_property(
    value: &str,
) -> Option<FontStyleDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_style(fallback)
            .map(|fallback| FontStyleDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(FontStyleDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_style_property(
    value: &str,
) -> Option<FontStyleDeclarationValue> {
    parse_font_style(value)
        .map(|value| match value {
            FontStyleValue::Normal => FontStyleDeclarationValue::Normal,
            FontStyleValue::Italic => FontStyleDeclarationValue::Italic,
        })
        .or_else(|| parse_font_style_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_style_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontStyleDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_style_property)
}

pub(in crate::native_engine::css) fn parse_font_stretch_custom_property(
    value: &str,
) -> Option<FontStretchDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback) => parse_font_stretch_value(fallback).map(|stretch| {
            FontStretchDeclarationValue::CustomPropertyFallback(
                name_hash,
                NativeFontStretchRange {
                    min: stretch,
                    max: stretch,
                },
            )
        }),
        None => Some(FontStretchDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_stretch_property(
    value: &str,
) -> Option<FontStretchDeclarationValue> {
    parse_font_stretch_value(value)
        .map(|stretch| {
            FontStretchDeclarationValue::Value(NativeFontStretchRange {
                min: stretch,
                max: stretch,
            })
        })
        .or_else(|| parse_font_stretch_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_font_stretch_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontStretchDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_stretch_property)
}

pub(in crate::native_engine::css) fn parse_font_family_var_arguments(
    value: &str,
) -> Option<(&str, Option<&str>)> {
    let mut depth = 0_usize;
    let mut comma = None;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => depth = depth.checked_add(1)?,
            b')' => {
                if depth == 0 {
                    return None;
                }
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 && comma.is_none() => comma = Some(index),
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let (name, fallback) = comma
        .map(|index| {
            (
                &value[..index],
                Some(value[index.saturating_add(1)..].trim()),
            )
        })
        .unwrap_or((value, None));
    let name = name.trim();
    if name.is_empty() || fallback.is_some_and(str::is_empty) {
        return None;
    }
    Some((name, fallback))
}

pub(in crate::native_engine::css) fn parse_font_family_custom_property(
    value: &str,
) -> Option<FontFamilyDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_family_var_arguments(arguments)?;
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
        Some(fallback) => parse_font_family(fallback).map(|fallback| {
            FontFamilyDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(FontFamilyDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_font_family_property(
    value: &str,
) -> Option<FontFamilyDeclarationValue> {
    let value = value.trim();
    let is_var = value
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("var("));
    if is_var {
        parse_font_family_custom_property(value)
    } else {
        parse_font_family(value).map(FontFamilyDeclarationValue::Value)
    }
}

pub(in crate::native_engine::css) fn parse_font_family_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<FontFamilyDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_family_property)
}

pub(in crate::native_engine::css) fn parse_font_size_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<NativeFontSizeDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_font_size_value)
}

pub(in crate::native_engine::css) fn parse_word_break_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<WordBreakDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_word_break_property)
}

pub(in crate::native_engine::css) fn parse_word_spacing_custom_property(
    value: &str,
) -> Option<WordSpacingDeclarationValue> {
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
        Some(fallback) => parse_dimension(fallback).map(|fallback| {
            WordSpacingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(WordSpacingDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_word_spacing_property(
    value: &str,
) -> Option<WordSpacingDeclarationValue> {
    parse_dimension(value)
        .map(WordSpacingDeclarationValue::Value)
        .or_else(|| parse_word_spacing_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_word_spacing_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<WordSpacingDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_word_spacing_property)
}

pub(in crate::native_engine::css) fn parse_letter_spacing_custom_property(
    value: &str,
) -> Option<LetterSpacingDeclarationValue> {
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
        Some(fallback) => parse_dimension(fallback).map(|fallback| {
            LetterSpacingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(LetterSpacingDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_letter_spacing_property(
    value: &str,
) -> Option<LetterSpacingDeclarationValue> {
    parse_dimension(value)
        .map(LetterSpacingDeclarationValue::Value)
        .or_else(|| parse_letter_spacing_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_letter_spacing_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<LetterSpacingDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_letter_spacing_property)
}

pub(in crate::native_engine::css) fn parse_vertical_align_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<VerticalAlignDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_vertical_align_property)
}

pub(in crate::native_engine::css) fn parse_text_overflow_custom_property(
    value: &str,
) -> Option<TextOverflowDeclarationValue> {
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
        Some(fallback) => parse_text_overflow(fallback).map(|fallback| {
            TextOverflowDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(TextOverflowDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_text_overflow_property(
    value: &str,
) -> Option<TextOverflowDeclarationValue> {
    parse_text_overflow(value)
        .map(TextOverflowDeclarationValue::Value)
        .or_else(|| parse_text_overflow_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_text_overflow_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<TextOverflowDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_text_overflow_property)
}

pub(in crate::native_engine::css) fn parse_text_indent_custom_property(
    value: &str,
) -> Option<TextIndentDeclarationValue> {
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
        Some(fallback) => parse_dimension(fallback).map(|fallback| {
            TextIndentDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(TextIndentDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_text_indent_property(
    value: &str,
) -> Option<TextIndentDeclarationValue> {
    parse_dimension(value)
        .map(TextIndentDeclarationValue::Value)
        .or_else(|| parse_text_indent_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_text_indent_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<TextIndentDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_text_indent_property)
}
