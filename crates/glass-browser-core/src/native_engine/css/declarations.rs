use super::*;

pub(in crate::native_engine) fn custom_property_hash(name: &str) -> u64 {
    let mut hash = 1469598103934665603u64;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

pub(super) fn parse_custom_property_name(property: &str) -> Option<u64> {
    let property = property.trim();
    if !property.starts_with("--")
        || property.len() <= 2
        || property.len() > MAX_NATIVE_CUSTOM_PROPERTY_NAME_BYTES
        || !property[2..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    Some(custom_property_hash(property))
}

pub(super) fn parse_custom_property_declarations(
    source: &str,
) -> Vec<NativeCustomPropertyDeclaration> {
    let mut declarations = Vec::new();
    for (order, (_, declaration)) in split_css_declarations(source).into_iter().enumerate() {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let Some(name_hash) = parse_custom_property_name(property) else {
            continue;
        };
        let (value, important) = strip_important_suffix(value);
        if value.len() > MAX_NATIVE_CUSTOM_PROPERTY_VALUE_BYTES {
            continue;
        }
        if declarations.len() >= MAX_NATIVE_CUSTOM_PROPERTIES_PER_RULE {
            break;
        }
        declarations.push(NativeCustomPropertyDeclaration {
            name_hash,
            value: value.to_owned(),
            important,
            order,
        });
    }
    declarations
}

pub(super) fn custom_property_candidate_precedes(
    candidate: &NativeCustomPropertyCandidate,
    current: &NativeCustomPropertyCandidate,
) -> bool {
    if candidate.important != current.important {
        return candidate.important;
    }
    if candidate.inline != current.inline {
        return candidate.inline;
    }
    if candidate.specificity != current.specificity {
        return candidate.specificity > current.specificity;
    }
    candidate.order >= current.order
}

pub(super) fn apply_custom_property_declarations(
    candidates: &mut BTreeMap<u64, NativeCustomPropertyCandidate>,
    declarations: &[NativeCustomPropertyDeclaration],
    specificity: u16,
    rule_order: usize,
    inline: bool,
) {
    for declaration in declarations {
        let candidate = NativeCustomPropertyCandidate {
            value: declaration.value.clone(),
            specificity,
            order: rule_order
                .saturating_mul(MAX_NATIVE_CUSTOM_PROPERTIES_PER_RULE + 1)
                .saturating_add(declaration.order),
            important: declaration.important,
            inline,
        };
        let replace = candidates
            .get(&declaration.name_hash)
            .is_none_or(|current| custom_property_candidate_precedes(&candidate, current));
        if replace {
            candidates.insert(declaration.name_hash, candidate);
        }
    }
}

pub(in crate::native_engine) fn parse_font_size_var_arguments(
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
            b',' if depth == 0 => {
                if comma.is_some() {
                    return None;
                }
                comma = Some(index);
            }
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

pub(super) fn parse_font_size_fallback_value(value: &str) -> Option<NativeFontSizeFallbackValue> {
    let value = value.trim();
    let keyword = value.to_ascii_lowercase();
    match keyword.as_str() {
        "inherit" => return Some(NativeFontSizeFallbackValue::Inherit),
        "initial" => return Some(NativeFontSizeFallbackValue::Initial),
        "unset" => return Some(NativeFontSizeFallbackValue::Unset),
        "revert" => return Some(NativeFontSizeFallbackValue::Revert),
        "revert-layer" => return Some(NativeFontSizeFallbackValue::RevertLayer),
        _ => {}
    }
    parse_font_size(value)
        .map(NativeFontSizeFallbackValue::Pixels)
        .or_else(|| {
            parse_relative_font_size(value).map(|value| match value {
                NativeFontSizeDeclarationValue::Relative(scale_milli) => {
                    NativeFontSizeFallbackValue::Relative(scale_milli)
                }
                NativeFontSizeDeclarationValue::RootRelative(scale_milli) => {
                    NativeFontSizeFallbackValue::RootRelative(scale_milli)
                }
                NativeFontSizeDeclarationValue::Viewport(scale_milli, unit) => {
                    NativeFontSizeFallbackValue::Viewport(scale_milli, unit)
                }
                _ => unreachable!("relative parser only emits relative values"),
            })
        })
        .or_else(|| {
            parse_font_size_calculation(value).map(NativeFontSizeFallbackValue::Calculation)
        })
}

pub(super) fn parse_font_size_custom_property(
    value: &str,
) -> Option<NativeFontSizeDeclarationValue> {
    let value = value.trim();
    if value.len() < 6
        || !value
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("var("))
        || !value.ends_with(')')
    {
        return None;
    }
    let arguments = value.get(4..value.len().saturating_sub(1))?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    let name_hash = parse_custom_property_name(name)?;
    fallback.map_or_else(
        || Some(NativeFontSizeDeclarationValue::CustomProperty(name_hash)),
        |fallback| {
            Some(NativeFontSizeDeclarationValue::CustomPropertyFallback(
                name_hash,
                parse_font_size_fallback_value(fallback)?,
            ))
        },
    )
}

pub(in crate::native_engine::css) fn parse_declarations_with_diagnostics(
    source: &str,
    diagnostic_source: NativeDiagnosticSource,
    base_offset: usize,
    diagnostics: &mut NativeDiagnosticSink,
) -> NativeDeclarations {
    let declarations = parse_declarations(source);
    for (declaration_offset, declaration) in split_css_declarations(source) {
        let offset = base_offset.saturating_add(declaration_offset);
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some((property, value)) = declaration.split_once(':') else {
            diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                diagnostic_source,
                offset,
                "missing-colon",
            );
            continue;
        };
        let property = property.trim();
        let value = value.trim();
        if value.is_empty() {
            diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                diagnostic_source,
                offset,
                "empty-value",
            );
            continue;
        }
        let (value, _) = strip_important_suffix(value);
        let property_name = property.to_ascii_lowercase();
        let supported = match property_name.as_str() {
            "display" => supports_display_declaration(value),
            "position" => parse_position_declaration(value).is_some(),
            "z-index" => parse_z_index_declaration(value).is_some(),
            "pointer-events" => parse_pointer_events_declaration(value).is_some(),
            "top" | "right" | "bottom" | "left" => {
                parse_position_offset_declaration(value).is_some()
            }
            "grid-template-columns" | "grid-template-rows" => {
                parse_grid_track_list_declaration(value).is_some()
            }
            "visibility" => parse_visibility_declaration(value).is_some(),
            "opacity" => parse_opacity_declaration(value).is_some(),
            "white-space" => parse_white_space_declaration(value).is_some(),
            "text-align" => parse_text_align_declaration(value).is_some(),
            "text-align-last" => parse_text_align_last_declaration(value).is_some(),
            "text-justify" => parse_text_justify_declaration(value).is_some(),
            "justify-content" => parse_justify_content_declaration(value).is_some(),
            "place-content" => parse_place_content_declaration(value).is_some(),
            "align-items" => parse_align_items_declaration(value).is_some(),
            "align-self" => parse_align_self_declaration(value).is_some(),
            "align-content" => parse_align_content_declaration(value).is_some(),
            "flex-direction" => parse_flex_direction_declaration(value).is_some(),
            "direction" => parse_direction_declaration(value).is_some(),
            "flex-wrap" => parse_flex_wrap_declaration(value).is_some(),
            "flex-flow" => parse_flex_flow_declaration(value).is_some(),
            "order" => parse_flex_item_order_declaration(value).is_some(),
            "flex" => parse_flex_shorthand_declaration(value).is_some(),
            "flex-grow" => parse_flex_grow_declaration(value).is_some(),
            "flex-shrink" => parse_flex_shrink_declaration(value).is_some(),
            "flex-basis" => parse_flex_basis_declaration(value).is_some(),
            "text-decoration" | "text-decoration-line" => {
                parse_text_decoration_declaration(value).is_some()
            }
            "text-decoration-style" => parse_text_decoration_style(value).is_some(),
            "text-decoration-skip-ink" => parse_text_decoration_skip_ink(value).is_some(),
            "text-decoration-skip-spaces" => parse_text_decoration_skip_spaces(value).is_some(),
            "text-decoration-thickness" => parse_text_decoration_thickness(value).is_some(),
            "text-underline-offset" => parse_text_underline_offset(value).is_some(),
            "text-decoration-color" => parse_text_decoration_color(value).is_some(),
            "text-transform" => parse_text_transform_declaration(value).is_some(),
            "font-variant" => parse_font_variant_shorthand_declaration(value).is_some(),
            "font-variant-ligatures" => parse_font_variant_ligatures_declaration(value).is_some(),
            "font-variant-caps" => parse_font_variant_caps_declaration(value).is_some(),
            "font-variant-position" => parse_font_variant_position_declaration(value).is_some(),
            "font-variant-alternates" => parse_font_variant_alternates_declaration(value).is_some(),
            "font-language-override" => parse_font_language_override_declaration(value).is_some(),
            "font-variation-settings" => parse_font_variation_settings_declaration(value).is_some(),
            "font-variant-east-asian" => parse_font_variant_east_asian_declaration(value).is_some(),
            "font-variant-numeric" => parse_font_variant_numeric_declaration(value).is_some(),
            "font-feature-settings" => parse_font_feature_settings_declaration(value).is_some(),
            "font-kerning" => parse_font_kerning_declaration(value).is_some(),
            "font-optical-sizing" => parse_font_optical_sizing_declaration(value).is_some(),
            "font-palette" => parse_font_palette_declaration(value).is_some(),
            "font-weight" => parse_font_weight_declaration(value).is_some(),
            "font-style" => parse_font_style_declaration(value).is_some(),
            "font-stretch" => parse_font_stretch_declaration(value).is_some(),
            "font-family" => parse_font_family_declaration(value).is_some(),
            "font-size" => parse_font_size_declaration(value).is_some(),
            "word-break" => parse_word_break_declaration(value).is_some(),
            "text-overflow" => parse_text_overflow_declaration(value).is_some(),
            "vertical-align" => parse_vertical_align_declaration(value).is_some(),
            "text-indent" => parse_text_indent_declaration(value).is_some(),
            "word-spacing" => parse_word_spacing_declaration(value).is_some(),
            "letter-spacing" => parse_letter_spacing_declaration(value).is_some(),
            "gap" => parse_gap_declaration(value).is_some(),
            "row-gap" | "column-gap" => parse_gap_component_declaration(value).is_some(),
            "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height" => {
                parse_local_dimension_declaration(value).is_some()
            }
            "line-height" => parse_line_height_declaration(value).is_some(),
            "background" => parse_background_shorthand_declaration(value).is_some(),
            "background-color" => parse_background_color_declaration(value).is_some(),
            "background-image" => parse_background_image_declaration(value).is_some(),
            "background-repeat" => parse_background_repeat_declaration(value).is_some(),
            "background-position" => parse_background_position_declaration(value).is_some(),
            "background-size" => parse_background_size_declaration(value).is_some(),
            "fill" | "stroke" => parse_svg_paint_declaration(value),
            "color" => parse_local_color_declaration(value).is_some(),
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                parse_border_declaration(value).is_some()
            }
            "border-block"
            | "border-block-start"
            | "border-block-end"
            | "border-inline"
            | "border-inline-start"
            | "border-inline-end" => parse_border_declaration(value).is_some(),
            "border-width" => parse_border_width_declaration(value).is_some(),
            "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width" => {
                parse_local_physical_border_width_side_declaration(value).is_some()
            }
            "border-block-width" | "border-inline-width" => {
                parse_logical_border_width_pair(value).is_some()
            }
            "border-block-start-width"
            | "border-block-end-width"
            | "border-inline-start-width"
            | "border-inline-end-width" => {
                parse_local_physical_border_width_side_declaration(value).is_some()
            }
            "border-style" => parse_border_style_declaration(value).is_some(),
            "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style" => {
                parse_local_physical_border_style_side_declaration(value).is_some()
            }
            "border-block-style" | "border-inline-style" => {
                parse_logical_border_style_pair(value).is_some()
            }
            "border-block-start-style"
            | "border-block-end-style"
            | "border-inline-start-style"
            | "border-inline-end-style" => {
                parse_local_physical_border_style_side_declaration(value).is_some()
            }
            "border-color" => parse_border_color_declaration(value).is_some(),
            "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color" => parse_border_color_side_declaration(value).is_some(),
            "border-block-color" | "border-inline-color" => {
                parse_logical_border_color_pair(value).is_some()
            }
            "border-block-start-color"
            | "border-block-end-color"
            | "border-inline-start-color"
            | "border-inline-end-color" => parse_border_color_side_declaration(value).is_some(),
            "border-radius" => parse_border_radius_declaration(value).is_some(),
            "border-top-left-radius"
            | "border-top-right-radius"
            | "border-bottom-right-radius"
            | "border-bottom-left-radius" => {
                parse_border_radius_corner_declaration(value).is_some()
            }
            "border-start-start-radius"
            | "border-start-end-radius"
            | "border-end-start-radius"
            | "border-end-end-radius" => parse_border_radius_corner_declaration(value).is_some(),
            "padding" => parse_local_box_edges(value).is_some(),
            "padding-block" | "padding-inline" => parse_local_box_edge_pair(value).is_some(),
            "padding-block-start"
            | "padding-block-end"
            | "padding-inline-start"
            | "padding-inline-end" => parse_local_padding_declaration(value).is_some(),
            "margin" => parse_local_margin_edges(value).is_some(),
            "margin-block" | "margin-inline" => parse_local_margin_edge_pair(value).is_some(),
            "margin-block-start"
            | "margin-block-end"
            | "margin-inline-start"
            | "margin-inline-end" => parse_local_margin_declaration(value).is_some(),
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                parse_local_padding_declaration(value).is_some()
            }
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
                parse_local_physical_margin_declaration(value).is_some()
            }
            "box-sizing" => parse_local_box_sizing_declaration(value).is_some(),
            "overflow" | "overflow-x" | "overflow-y" => parse_overflow_declaration(value)
                .is_some_and(|value| {
                    matches!(
                        value,
                        LocalCascadeDeclaration::Inherit
                            | LocalCascadeDeclaration::Reset
                            | LocalCascadeDeclaration::RevertLayer
                            | LocalCascadeDeclaration::Value(
                                OverflowDeclarationValue::Value(
                                    OverflowValue::Hidden
                                        | OverflowValue::Clip
                                        | OverflowValue::Auto
                                        | OverflowValue::Scroll
                                        | OverflowValue::Other,
                                ) | OverflowDeclarationValue::CustomProperty(_)
                                    | OverflowDeclarationValue::CustomPropertyFallback(_, _),
                            )
                    )
                }),
            _ => {
                diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssProperty,
                    diagnostic_source,
                    offset,
                    property,
                );
                false
            }
        };
        if !supported && is_known_css_property(property_name.as_str()) {
            diagnostics.push(
                NativeDiagnosticCode::UnsupportedCssValue,
                diagnostic_source,
                offset,
                property_name.as_str(),
            );
        }
    }
    declarations
}

pub(crate) fn collect_declaration_diagnostics(
    source: &str,
    diagnostic_source: NativeDiagnosticSource,
    base_offset: usize,
    diagnostics: &mut NativeDiagnosticSink,
) {
    let _ =
        parse_declarations_with_diagnostics(source, diagnostic_source, base_offset, diagnostics);
}

pub(super) fn is_known_css_property(property: &str) -> bool {
    matches!(
        property,
        "display"
            | "position"
            | "z-index"
            | "pointer-events"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "grid-template-columns"
            | "grid-template-rows"
            | "visibility"
            | "opacity"
            | "white-space"
            | "text-align"
            | "text-align-last"
            | "text-justify"
            | "justify-content"
            | "place-content"
            | "align-items"
            | "align-self"
            | "align-content"
            | "flex-direction"
            | "direction"
            | "flex-wrap"
            | "flex-flow"
            | "order"
            | "flex"
            | "flex-grow"
            | "flex-shrink"
            | "flex-basis"
            | "text-decoration"
            | "text-decoration-line"
            | "text-decoration-style"
            | "text-decoration-skip-ink"
            | "text-decoration-skip-spaces"
            | "text-decoration-thickness"
            | "text-underline-offset"
            | "text-decoration-color"
            | "text-transform"
            | "font-variant"
            | "font-variant-ligatures"
            | "font-variant-caps"
            | "font-variant-position"
            | "font-variant-alternates"
            | "font-language-override"
            | "font-variation-settings"
            | "font-variant-east-asian"
            | "font-variant-numeric"
            | "font-feature-settings"
            | "font-kerning"
            | "font-optical-sizing"
            | "font-palette"
            | "font-weight"
            | "font-style"
            | "font-stretch"
            | "word-break"
            | "text-overflow"
            | "vertical-align"
            | "text-indent"
            | "gap"
            | "row-gap"
            | "column-gap"
            | "width"
            | "height"
            | "min-width"
            | "max-width"
            | "min-height"
            | "max-height"
            | "line-height"
            | "background"
            | "background-color"
            | "background-image"
            | "background-repeat"
            | "background-position"
            | "background-size"
            | "fill"
            | "stroke"
            | "color"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-block"
            | "border-block-start"
            | "border-block-end"
            | "border-inline"
            | "border-inline-start"
            | "border-inline-end"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "border-block-width"
            | "border-block-start-width"
            | "border-block-end-width"
            | "border-inline-width"
            | "border-inline-start-width"
            | "border-inline-end-width"
            | "border-style"
            | "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style"
            | "border-block-style"
            | "border-block-start-style"
            | "border-block-end-style"
            | "border-inline-style"
            | "border-inline-start-style"
            | "border-inline-end-style"
            | "border-color"
            | "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color"
            | "border-block-color"
            | "border-block-start-color"
            | "border-block-end-color"
            | "border-inline-color"
            | "border-inline-start-color"
            | "border-inline-end-color"
            | "border-radius"
            | "border-top-left-radius"
            | "border-top-right-radius"
            | "border-bottom-right-radius"
            | "border-bottom-left-radius"
            | "border-start-start-radius"
            | "border-start-end-radius"
            | "border-end-start-radius"
            | "border-end-end-radius"
            | "padding"
            | "padding-block"
            | "padding-block-start"
            | "padding-block-end"
            | "padding-inline"
            | "padding-inline-start"
            | "padding-inline-end"
            | "margin"
            | "margin-block"
            | "margin-block-start"
            | "margin-block-end"
            | "margin-inline"
            | "margin-inline-start"
            | "margin-inline-end"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "box-sizing"
            | "overflow"
            | "overflow-x"
            | "overflow-y"
    )
}

pub(super) fn split_css_declarations(source: &str) -> Vec<(usize, &str)> {
    let mut declarations = Vec::new();
    let mut start = 0;
    let mut parentheses = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in source.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(delimiter) = quote {
            if byte == b'\\' {
                escaped = true;
            } else if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => parentheses = parentheses.saturating_add(1),
            b')' => parentheses = parentheses.saturating_sub(1),
            b';' if parentheses == 0 => {
                declarations.push((start, &source[start..index]));
                start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    declarations.push((start, &source[start..]));
    declarations
}

pub(super) fn selector_diagnostic_detail(selector: &str) -> &'static str {
    let selector = selector.trim();
    if selector.is_empty() {
        "empty-selector"
    } else if selector.len() > MAX_SELECTOR_BYTES {
        "selector-too-long"
    } else if split_selector_compounds(selector)
        .is_some_and(|compounds| compounds.len() > MAX_SELECTOR_PARTS)
    {
        "selector-too-complex"
    } else if selector.contains(':') {
        "pseudo-selector"
    } else if selector.contains(['>', '+', '~']) {
        "selector-combinator"
    } else {
        "invalid-selector"
    }
}

pub(super) fn parse_declarations(source: &str) -> NativeDeclarations {
    let mut declarations = NativeDeclarations::default();
    for (declaration_order, (_, declaration)) in
        split_css_declarations(source).into_iter().enumerate()
    {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let (value, important) = strip_important_suffix(value);
        match property.to_ascii_lowercase().as_str() {
            "display" => {
                if let Some(parsed) = parse_display_declaration(value) {
                    declarations.display = Some(parsed);
                    declarations.local_importance.display = important;
                }
            }
            "position" => {
                if let Some(parsed) = parse_position_declaration(value) {
                    declarations.position = Some(parsed);
                    declarations.local_importance.position = important;
                }
            }
            "z-index" => {
                if let Some(parsed) = parse_z_index_declaration(value) {
                    declarations.z_index = Some(parsed);
                    declarations.local_importance.z_index = important;
                }
            }
            "pointer-events" => {
                if let Some(parsed) = parse_pointer_events_declaration(value) {
                    declarations.pointer_events = Some(parsed);
                    declarations.text_importance.pointer_events = important;
                }
            }
            "top" => {
                if let Some(parsed) = parse_position_offset_declaration(value) {
                    declarations.top = Some(parsed);
                    declarations.local_importance.top = important;
                }
            }
            "right" => {
                if let Some(parsed) = parse_position_offset_declaration(value) {
                    declarations.right = Some(parsed);
                    declarations.local_importance.right = important;
                }
            }
            "bottom" => {
                if let Some(parsed) = parse_position_offset_declaration(value) {
                    declarations.bottom = Some(parsed);
                    declarations.local_importance.bottom = important;
                }
            }
            "left" => {
                if let Some(parsed) = parse_position_offset_declaration(value) {
                    declarations.left = Some(parsed);
                    declarations.local_importance.left = important;
                }
            }
            "grid-template-columns" => {
                if let Some(parsed) = parse_grid_track_list_declaration(value) {
                    declarations.grid_template_columns = Some(parsed);
                    declarations.grid_template_columns_important = important;
                }
            }
            "grid-template-rows" => {
                if let Some(parsed) = parse_grid_track_list_declaration(value) {
                    declarations.grid_template_rows = Some(parsed);
                    declarations.grid_template_rows_important = important;
                }
            }
            "visibility" => {
                if let Some(parsed) = parse_visibility_declaration(value) {
                    declarations.visibility = Some(parsed);
                    declarations.local_importance.visibility = important;
                }
            }
            "opacity" => {
                if let Some(parsed) = parse_opacity_declaration(value) {
                    declarations.opacity = Some(parsed);
                    declarations.local_importance.opacity = important;
                }
            }
            "white-space" => {
                if let Some(parsed) = parse_white_space_declaration(value) {
                    declarations.white_space = Some(parsed);
                    declarations.text_importance.white_space = important;
                }
            }
            "text-align" => {
                if let Some(parsed) = parse_text_align_declaration(value) {
                    declarations.text_align = Some(parsed);
                    declarations.text_importance.text_align = important;
                }
            }
            "text-align-last" => {
                if let Some(parsed) = parse_text_align_last_declaration(value) {
                    declarations.text_align_last = Some(parsed);
                    declarations.text_importance.text_align_last = important;
                }
            }
            "text-justify" => {
                if let Some(parsed) = parse_text_justify_declaration(value) {
                    declarations.text_justify = Some(parsed);
                    declarations.text_importance.text_justify = important;
                }
            }
            "justify-content" => {
                if let Some(parsed) = parse_justify_content_declaration(value) {
                    declarations.justify_content = Some(parsed);
                    declarations.flex_importance.justify_content = important;
                }
            }
            "place-content" => {
                if let Some((align_content, justify_content)) =
                    parse_place_content_declaration(value)
                {
                    declarations.align_content = Some(align_content);
                    declarations.justify_content = Some(justify_content);
                    declarations.flex_importance.align_content = important;
                    declarations.flex_importance.justify_content = important;
                }
            }
            "align-items" => {
                if let Some(parsed) = parse_align_items_declaration(value) {
                    declarations.align_items = Some(parsed);
                    declarations.flex_importance.align_items = important;
                }
            }
            "align-self" => {
                if let Some(parsed) = parse_align_self_declaration(value) {
                    declarations.align_self = Some(parsed);
                    declarations.flex_importance.align_self = important;
                }
            }
            "align-content" => {
                if let Some(parsed) = parse_align_content_declaration(value) {
                    declarations.align_content = Some(parsed);
                    declarations.flex_importance.align_content = important;
                }
            }
            "flex-direction" => {
                if let Some(parsed) = parse_flex_direction_declaration(value) {
                    declarations.flex_direction = Some(parsed);
                    declarations.flex_importance.flex_direction = important;
                }
            }
            "direction" => {
                if let Some(parsed) = parse_direction_declaration(value) {
                    declarations.direction = Some(parsed);
                    declarations.text_importance.direction = important;
                }
            }
            "flex-wrap" => {
                if let Some(parsed) = parse_flex_wrap_declaration(value) {
                    declarations.flex_wrap = Some(parsed);
                    declarations.flex_importance.flex_wrap = important;
                }
            }
            "flex-flow" => {
                if let Some((direction, wrap)) = parse_flex_flow_declaration(value) {
                    declarations.flex_direction = Some(direction);
                    declarations.flex_wrap = Some(wrap);
                    declarations.flex_importance.flex_direction = important;
                    declarations.flex_importance.flex_wrap = important;
                }
            }
            "order" => {
                if let Some(parsed) = parse_flex_item_order_declaration(value) {
                    declarations.order = Some(parsed);
                    declarations.flex_importance.order = important;
                }
            }
            "flex" => {
                if let Some((grow, shrink, basis)) = parse_flex_shorthand_declaration(value) {
                    declarations.flex_grow = Some(grow);
                    declarations.flex_shrink = Some(shrink);
                    declarations.flex_basis = Some(basis);
                    declarations.flex_importance.flex_grow = important;
                    declarations.flex_importance.flex_shrink = important;
                    declarations.flex_importance.flex_basis = important;
                }
            }
            "flex-grow" => {
                if let Some(parsed) = parse_flex_grow_declaration(value) {
                    declarations.flex_grow = Some(parsed);
                    declarations.flex_importance.flex_grow = important;
                }
            }
            "flex-shrink" => {
                if let Some(parsed) = parse_flex_shrink_declaration(value) {
                    declarations.flex_shrink = Some(parsed);
                    declarations.flex_importance.flex_shrink = important;
                }
            }
            "flex-basis" => {
                if let Some(parsed) = parse_flex_basis_declaration(value) {
                    declarations.flex_basis = Some(parsed);
                    declarations.flex_importance.flex_basis = important;
                }
            }
            "text-decoration" | "text-decoration-line" => {
                if let Some(parsed) = parse_text_decoration_declaration(value) {
                    declarations.text_decoration = Some(parsed);
                    declarations.text_importance.text_decoration = important;
                }
            }
            "text-decoration-style" => {
                if let Some(parsed) = parse_text_decoration_style(value) {
                    declarations.text_decoration_style = Some(parsed);
                    declarations.text_importance.text_decoration_style = important;
                }
            }
            "text-decoration-skip-ink" => {
                if let Some(parsed) = parse_text_decoration_skip_ink(value) {
                    declarations.text_decoration_skip_ink = Some(parsed);
                    declarations.text_importance.text_decoration_skip_ink = important;
                }
            }
            "text-decoration-skip-spaces" => {
                if let Some(parsed) = parse_text_decoration_skip_spaces(value) {
                    declarations.text_decoration_skip_spaces = Some(parsed);
                    declarations.text_importance.text_decoration_skip_spaces = important;
                }
            }
            "text-decoration-thickness" => {
                if let Some(parsed) = parse_text_decoration_thickness(value) {
                    declarations.text_decoration_thickness = Some(parsed);
                    declarations.text_importance.text_decoration_thickness = important;
                }
            }
            "text-underline-offset" => {
                if let Some(parsed) = parse_text_underline_offset(value) {
                    declarations.text_underline_offset = Some(parsed);
                    declarations.text_importance.text_underline_offset = important;
                }
            }
            "text-decoration-color" => {
                if let Some(parsed) = parse_text_decoration_color(value) {
                    declarations.text_decoration_color = Some(parsed);
                    declarations.text_decoration_color_important = important;
                }
            }
            "text-transform" => {
                if let Some(parsed) = parse_text_transform_declaration(value) {
                    declarations.text_transform = Some(parsed);
                    declarations.text_importance.text_transform = important;
                }
            }
            "font-variant" => {
                if let Some(parsed) = parse_font_variant_shorthand_declaration(value) {
                    declarations.font_variant_ligatures =
                        Some(wrap_font_variant_ligatures_declaration(parsed.ligatures));
                    declarations.font_variant_caps =
                        Some(wrap_font_variant_caps_declaration(parsed.caps));
                    declarations.font_variant_position =
                        Some(wrap_font_variant_position_declaration(parsed.position));
                    declarations.font_variant_alternates =
                        Some(wrap_font_variant_alternates_declaration(parsed.alternates));
                    declarations.font_variant_east_asian =
                        Some(wrap_font_variant_east_asian_declaration(parsed.east_asian));
                    declarations.font_variant_numeric =
                        Some(wrap_font_variant_numeric_declaration(parsed.numeric));
                    declarations.text_importance.font_variant_ligatures = important;
                    declarations.text_importance.font_variant_caps = important;
                    declarations.text_importance.font_variant_position = important;
                    declarations.text_importance.font_variant_alternates = important;
                    declarations.text_importance.font_variant_east_asian = important;
                    declarations.text_importance.font_variant_numeric = important;
                } else if let Some(parsed) = parse_font_variant_shorthand_custom_property(value) {
                    declarations.font_variant_ligatures = Some(parsed.ligatures);
                    declarations.font_variant_caps = Some(parsed.caps);
                    declarations.font_variant_position = Some(parsed.position);
                    declarations.font_variant_alternates = Some(parsed.alternates);
                    declarations.font_variant_east_asian = Some(parsed.east_asian);
                    declarations.font_variant_numeric = Some(parsed.numeric);
                    declarations.text_importance.font_variant_ligatures = important;
                    declarations.text_importance.font_variant_caps = important;
                    declarations.text_importance.font_variant_position = important;
                    declarations.text_importance.font_variant_alternates = important;
                    declarations.text_importance.font_variant_east_asian = important;
                    declarations.text_importance.font_variant_numeric = important;
                }
            }
            "font-variant-ligatures" => {
                if let Some(parsed) = parse_font_variant_ligatures_declaration(value) {
                    declarations.font_variant_ligatures = Some(parsed);
                    declarations.text_importance.font_variant_ligatures = important;
                }
            }
            "font-language-override" => {
                if let Some(parsed) = parse_font_language_override_declaration(value) {
                    declarations.font_language_override = Some(parsed);
                    declarations.text_importance.font_language_override = important;
                }
            }
            "font-variation-settings" => {
                if let Some(parsed) = parse_font_variation_settings_declaration(value) {
                    declarations.font_variation_settings = Some(parsed);
                    declarations.text_importance.font_variation_settings = important;
                }
            }
            "font-variant-caps" => {
                if let Some(parsed) = parse_font_variant_caps_declaration(value) {
                    declarations.font_variant_caps = Some(parsed);
                    declarations.text_importance.font_variant_caps = important;
                }
            }
            "font-variant-position" => {
                if let Some(parsed) = parse_font_variant_position_declaration(value) {
                    declarations.font_variant_position = Some(parsed);
                    declarations.text_importance.font_variant_position = important;
                }
            }
            "font-variant-alternates" => {
                if let Some(parsed) = parse_font_variant_alternates_declaration(value) {
                    declarations.font_variant_alternates = Some(parsed);
                    declarations.text_importance.font_variant_alternates = important;
                }
            }
            "font-variant-east-asian" => {
                if let Some(parsed) = parse_font_variant_east_asian_declaration(value) {
                    declarations.font_variant_east_asian = Some(parsed);
                    declarations.text_importance.font_variant_east_asian = important;
                }
            }
            "font-variant-numeric" => {
                if let Some(parsed) = parse_font_variant_numeric_declaration(value) {
                    declarations.font_variant_numeric = Some(parsed);
                    declarations.text_importance.font_variant_numeric = important;
                }
            }
            "font-feature-settings" => {
                if let Some(parsed) = parse_font_feature_settings_declaration(value) {
                    declarations.font_feature_settings = Some(parsed);
                    declarations.text_importance.font_feature_settings = important;
                }
            }
            "font-kerning" => {
                if let Some(parsed) = parse_font_kerning_declaration(value) {
                    declarations.font_kerning = Some(parsed);
                    declarations.text_importance.font_kerning = important;
                }
            }
            "font-optical-sizing" => {
                if let Some(parsed) = parse_font_optical_sizing_declaration(value) {
                    declarations.font_optical_sizing = Some(parsed);
                    declarations.text_importance.font_optical_sizing = important;
                }
            }
            "font-palette" => {
                if let Some(parsed) = parse_font_palette_declaration(value) {
                    declarations.font_palette = Some(parsed);
                    declarations.text_importance.font_palette = important;
                }
            }
            "font-weight" => {
                if let Some(parsed) = parse_font_weight_declaration(value) {
                    declarations.font_weight = Some(parsed);
                    declarations.text_importance.font_weight = important;
                }
            }
            "font-style" => {
                if let Some(parsed) = parse_font_style_declaration(value) {
                    declarations.font_style = Some(parsed);
                    declarations.text_importance.font_style = important;
                }
            }
            "font-stretch" => {
                if let Some(parsed) = parse_font_stretch_declaration(value) {
                    declarations.font_stretch = Some(parsed);
                    declarations.text_importance.font_stretch = important;
                }
            }
            "font-family" => {
                if let Some(parsed) = parse_font_family_declaration(value) {
                    declarations.font_family = Some(parsed);
                    declarations.text_importance.font_family = important;
                }
            }
            "font-size" => {
                if let Some(parsed) = parse_font_size_declaration(value) {
                    declarations.font_size = Some(parsed);
                    declarations.text_importance.font_size = important;
                }
            }
            "word-break" => {
                if let Some(parsed) = parse_word_break_declaration(value) {
                    declarations.word_break = Some(parsed);
                    declarations.text_importance.word_break = important;
                }
            }
            "text-overflow" => {
                if let Some(parsed) = parse_text_overflow_declaration(value) {
                    declarations.text_overflow = Some(parsed);
                    declarations.text_importance.text_overflow = important;
                }
            }
            "vertical-align" => {
                if let Some(parsed) = parse_vertical_align_declaration(value) {
                    declarations.vertical_align = Some(parsed);
                    declarations.text_importance.vertical_align = important;
                }
            }
            "text-indent" => {
                if let Some(parsed) = parse_text_indent_declaration(value) {
                    declarations.text_indent = Some(parsed);
                    declarations.text_importance.text_indent = important;
                }
            }
            "word-spacing" => {
                if let Some(parsed) = parse_word_spacing_declaration(value) {
                    declarations.word_spacing = Some(parsed);
                    declarations.text_importance.word_spacing = important;
                }
            }
            "letter-spacing" => {
                if let Some(parsed) = parse_letter_spacing_declaration(value) {
                    declarations.letter_spacing = Some(parsed);
                    declarations.text_importance.letter_spacing = important;
                }
            }
            "gap" => {
                if let Some(parsed) = parse_gap_declaration(value) {
                    declarations.gap = Some(parsed);
                    declarations.gap_order = declaration_order;
                    declarations.gap_important = important;
                }
            }
            "row-gap" => {
                if let Some(parsed) = parse_gap_component_declaration(value) {
                    declarations.row_gap = Some(parsed);
                    declarations.row_gap_order = declaration_order;
                    declarations.row_gap_important = important;
                }
            }
            "column-gap" => {
                if let Some(parsed) = parse_gap_component_declaration(value) {
                    declarations.column_gap = Some(parsed);
                    declarations.column_gap_order = declaration_order;
                    declarations.column_gap_important = important;
                }
            }
            "width" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.width = Some(parsed);
                    declarations.dimension_importance.width = important;
                }
            }
            "height" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.height = Some(parsed);
                    declarations.dimension_importance.height = important;
                }
            }
            "min-width" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.min_width = Some(parsed);
                    declarations.dimension_importance.min_width = important;
                }
            }
            "max-width" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.max_width = Some(parsed);
                    declarations.dimension_importance.max_width = important;
                }
            }
            "min-height" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.min_height = Some(parsed);
                    declarations.dimension_importance.min_height = important;
                }
            }
            "max-height" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.max_height = Some(parsed);
                    declarations.dimension_importance.max_height = important;
                }
            }
            "line-height" => {
                if let Some(parsed) = parse_line_height_declaration(value) {
                    declarations.line_height = Some(parsed);
                    declarations.text_importance.line_height = important;
                }
            }
            "background" => {
                if let Some(parsed) = parse_background_shorthand_declaration(value) {
                    apply_background_shorthand_declaration(&mut declarations, parsed, important);
                }
            }
            "background-color" => {
                if let Some(parsed) = parse_background_color_declaration(value) {
                    declarations.background_color = Some(parsed);
                    declarations.background_color_important = important;
                }
            }
            "background-image" => {
                if let Some(parsed) = parse_background_image_declaration(value) {
                    declarations.background_image = Some(parsed);
                    declarations.background_image_important = important;
                }
            }
            "background-repeat" => {
                if let Some(parsed) = parse_background_repeat_declaration(value) {
                    declarations.background_repeat = Some(parsed);
                    declarations.background_repeat_important = important;
                }
            }
            "background-position" => {
                if let Some(parsed) = parse_background_position_declaration(value) {
                    declarations.background_position = Some(parsed);
                    declarations.background_position_important = important;
                }
            }
            "background-size" => {
                if let Some(parsed) = parse_background_size_declaration(value) {
                    declarations.background_size = Some(parsed);
                    declarations.background_size_important = important;
                }
            }
            "border" => {
                if let Some(border) = parse_border_declaration(value) {
                    declarations.border = [Some(border); 4];
                    declarations.border_order = [declaration_order; 4];
                    declarations.border_important = [important; 4];
                }
            }
            "border-top" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    &mut declarations.border_important,
                    0,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-right" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    &mut declarations.border_important,
                    1,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-bottom" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    &mut declarations.border_important,
                    2,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-left" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    &mut declarations.border_important,
                    3,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-block" => {
                set_logical_border_pair(
                    &mut declarations.logical_border.border,
                    &mut declarations.logical_border.border_order,
                    &mut declarations.logical_border.border_important,
                    [LOGICAL_BORDER_BLOCK_START, LOGICAL_BORDER_BLOCK_END],
                    parse_border_declaration(value).map(|parsed| [parsed; 2]),
                    declaration_order,
                    important,
                );
            }
            "border-block-start" => {
                set_logical_border_side(
                    &mut declarations.logical_border.border,
                    &mut declarations.logical_border.border_order,
                    &mut declarations.logical_border.border_important,
                    LOGICAL_BORDER_BLOCK_START,
                    parse_border_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-end" => {
                set_logical_border_side(
                    &mut declarations.logical_border.border,
                    &mut declarations.logical_border.border_order,
                    &mut declarations.logical_border.border_important,
                    LOGICAL_BORDER_BLOCK_END,
                    parse_border_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline" => {
                set_logical_border_pair(
                    &mut declarations.logical_border.border,
                    &mut declarations.logical_border.border_order,
                    &mut declarations.logical_border.border_important,
                    [LOGICAL_BORDER_INLINE_START, LOGICAL_BORDER_INLINE_END],
                    parse_border_declaration(value).map(|parsed| [parsed; 2]),
                    declaration_order,
                    important,
                );
            }
            "border-inline-start" => {
                set_logical_border_side(
                    &mut declarations.logical_border.border,
                    &mut declarations.logical_border.border_order,
                    &mut declarations.logical_border.border_important,
                    LOGICAL_BORDER_INLINE_START,
                    parse_border_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-end" => {
                set_logical_border_side(
                    &mut declarations.logical_border.border,
                    &mut declarations.logical_border.border_order,
                    &mut declarations.logical_border.border_important,
                    LOGICAL_BORDER_INLINE_END,
                    parse_border_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-width" => {
                if let Some(values) = parse_border_width_declaration(value) {
                    declarations.border_width = values.map(Some);
                    declarations.border_width_order = [declaration_order; 4];
                    declarations.border_width_important = [important; 4];
                }
            }
            "border-top-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    &mut declarations.border_width_important,
                    0,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-right-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    &mut declarations.border_width_important,
                    1,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-bottom-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    &mut declarations.border_width_important,
                    2,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-left-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    &mut declarations.border_width_important,
                    3,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-block-width" => {
                set_logical_border_width_pair(
                    &mut declarations.logical_border.width,
                    &mut declarations.logical_border.width_order,
                    &mut declarations.logical_border.width_important,
                    [LOGICAL_BORDER_BLOCK_START, LOGICAL_BORDER_BLOCK_END],
                    parse_logical_border_width_pair(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-start-width" => {
                set_logical_border_width_side(
                    &mut declarations.logical_border.width,
                    &mut declarations.logical_border.width_order,
                    &mut declarations.logical_border.width_important,
                    LOGICAL_BORDER_BLOCK_START,
                    parse_local_physical_border_width_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-end-width" => {
                set_logical_border_width_side(
                    &mut declarations.logical_border.width,
                    &mut declarations.logical_border.width_order,
                    &mut declarations.logical_border.width_important,
                    LOGICAL_BORDER_BLOCK_END,
                    parse_local_physical_border_width_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-width" => {
                set_logical_border_width_pair(
                    &mut declarations.logical_border.width,
                    &mut declarations.logical_border.width_order,
                    &mut declarations.logical_border.width_important,
                    [LOGICAL_BORDER_INLINE_START, LOGICAL_BORDER_INLINE_END],
                    parse_logical_border_width_pair(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-start-width" => {
                set_logical_border_width_side(
                    &mut declarations.logical_border.width,
                    &mut declarations.logical_border.width_order,
                    &mut declarations.logical_border.width_important,
                    LOGICAL_BORDER_INLINE_START,
                    parse_local_physical_border_width_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-end-width" => {
                set_logical_border_width_side(
                    &mut declarations.logical_border.width,
                    &mut declarations.logical_border.width_order,
                    &mut declarations.logical_border.width_important,
                    LOGICAL_BORDER_INLINE_END,
                    parse_local_physical_border_width_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-style" => {
                if let Some(values) = parse_border_style_declaration(value) {
                    declarations.border_style = values.map(Some);
                    declarations.border_style_order = [declaration_order; 4];
                    declarations.border_style_important = [important; 4];
                }
            }
            "border-top-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    &mut declarations.border_style_important,
                    0,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-right-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    &mut declarations.border_style_important,
                    1,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-bottom-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    &mut declarations.border_style_important,
                    2,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-left-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    &mut declarations.border_style_important,
                    3,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-block-style" => {
                set_logical_border_style_pair(
                    &mut declarations.logical_border.style,
                    &mut declarations.logical_border.style_order,
                    &mut declarations.logical_border.style_important,
                    [LOGICAL_BORDER_BLOCK_START, LOGICAL_BORDER_BLOCK_END],
                    parse_logical_border_style_pair(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-start-style" => {
                set_logical_border_style_side(
                    &mut declarations.logical_border.style,
                    &mut declarations.logical_border.style_order,
                    &mut declarations.logical_border.style_important,
                    LOGICAL_BORDER_BLOCK_START,
                    parse_local_physical_border_style_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-end-style" => {
                set_logical_border_style_side(
                    &mut declarations.logical_border.style,
                    &mut declarations.logical_border.style_order,
                    &mut declarations.logical_border.style_important,
                    LOGICAL_BORDER_BLOCK_END,
                    parse_local_physical_border_style_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-style" => {
                set_logical_border_style_pair(
                    &mut declarations.logical_border.style,
                    &mut declarations.logical_border.style_order,
                    &mut declarations.logical_border.style_important,
                    [LOGICAL_BORDER_INLINE_START, LOGICAL_BORDER_INLINE_END],
                    parse_logical_border_style_pair(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-start-style" => {
                set_logical_border_style_side(
                    &mut declarations.logical_border.style,
                    &mut declarations.logical_border.style_order,
                    &mut declarations.logical_border.style_important,
                    LOGICAL_BORDER_INLINE_START,
                    parse_local_physical_border_style_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-end-style" => {
                set_logical_border_style_side(
                    &mut declarations.logical_border.style,
                    &mut declarations.logical_border.style_order,
                    &mut declarations.logical_border.style_important,
                    LOGICAL_BORDER_INLINE_END,
                    parse_local_physical_border_style_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-color" => {
                if let Some(values) = parse_border_color_declaration(value) {
                    declarations.border_color = values.map(Some);
                    declarations.border_color_order = [declaration_order; 4];
                    declarations.border_color_important = [important; 4];
                }
            }
            "border-top-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    &mut declarations.border_color_important,
                    0,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-right-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    &mut declarations.border_color_important,
                    1,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-bottom-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    &mut declarations.border_color_important,
                    2,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-left-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    &mut declarations.border_color_important,
                    3,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-block-color" => {
                set_logical_border_color_pair(
                    &mut declarations.logical_border.color,
                    &mut declarations.logical_border.color_order,
                    &mut declarations.logical_border.color_important,
                    [LOGICAL_BORDER_BLOCK_START, LOGICAL_BORDER_BLOCK_END],
                    parse_logical_border_color_pair(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-start-color" => {
                set_logical_border_color_side(
                    &mut declarations.logical_border.color,
                    &mut declarations.logical_border.color_order,
                    &mut declarations.logical_border.color_important,
                    LOGICAL_BORDER_BLOCK_START,
                    parse_border_color_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-block-end-color" => {
                set_logical_border_color_side(
                    &mut declarations.logical_border.color,
                    &mut declarations.logical_border.color_order,
                    &mut declarations.logical_border.color_important,
                    LOGICAL_BORDER_BLOCK_END,
                    parse_border_color_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-color" => {
                set_logical_border_color_pair(
                    &mut declarations.logical_border.color,
                    &mut declarations.logical_border.color_order,
                    &mut declarations.logical_border.color_important,
                    [LOGICAL_BORDER_INLINE_START, LOGICAL_BORDER_INLINE_END],
                    parse_logical_border_color_pair(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-start-color" => {
                set_logical_border_color_side(
                    &mut declarations.logical_border.color,
                    &mut declarations.logical_border.color_order,
                    &mut declarations.logical_border.color_important,
                    LOGICAL_BORDER_INLINE_START,
                    parse_border_color_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-inline-end-color" => {
                set_logical_border_color_side(
                    &mut declarations.logical_border.color,
                    &mut declarations.logical_border.color_order,
                    &mut declarations.logical_border.color_important,
                    LOGICAL_BORDER_INLINE_END,
                    parse_border_color_side_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-radius" => {
                if let Some(parsed) = parse_border_radius_declaration(value) {
                    declarations.border_radius = Some(parsed);
                    declarations.border_radius_order = declaration_order;
                    declarations.border_radius_important = important;
                }
            }
            "border-top-left-radius" => {
                set_border_radius_corner(
                    &mut declarations.border_radius_corners,
                    &mut declarations.border_radius_corner_orders,
                    &mut declarations.border_radius_corner_important,
                    0,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-top-right-radius" => {
                set_border_radius_corner(
                    &mut declarations.border_radius_corners,
                    &mut declarations.border_radius_corner_orders,
                    &mut declarations.border_radius_corner_important,
                    1,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-bottom-right-radius" => {
                set_border_radius_corner(
                    &mut declarations.border_radius_corners,
                    &mut declarations.border_radius_corner_orders,
                    &mut declarations.border_radius_corner_important,
                    2,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-bottom-left-radius" => {
                set_border_radius_corner(
                    &mut declarations.border_radius_corners,
                    &mut declarations.border_radius_corner_orders,
                    &mut declarations.border_radius_corner_important,
                    3,
                    value,
                    declaration_order,
                    important,
                );
            }
            "border-start-start-radius" => {
                set_logical_border_radius_corner(
                    &mut declarations.logical_border_radius.corners,
                    &mut declarations.logical_border_radius.corner_orders,
                    &mut declarations.logical_border_radius.important,
                    LOGICAL_BORDER_RADIUS_START_START,
                    parse_border_radius_corner_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-start-end-radius" => {
                set_logical_border_radius_corner(
                    &mut declarations.logical_border_radius.corners,
                    &mut declarations.logical_border_radius.corner_orders,
                    &mut declarations.logical_border_radius.important,
                    LOGICAL_BORDER_RADIUS_START_END,
                    parse_border_radius_corner_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-end-start-radius" => {
                set_logical_border_radius_corner(
                    &mut declarations.logical_border_radius.corners,
                    &mut declarations.logical_border_radius.corner_orders,
                    &mut declarations.logical_border_radius.important,
                    LOGICAL_BORDER_RADIUS_END_START,
                    parse_border_radius_corner_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "border-end-end-radius" => {
                set_logical_border_radius_corner(
                    &mut declarations.logical_border_radius.corners,
                    &mut declarations.logical_border_radius.corner_orders,
                    &mut declarations.logical_border_radius.important,
                    LOGICAL_BORDER_RADIUS_END_END,
                    parse_border_radius_corner_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "padding" => {
                if let Some(values) = parse_local_box_edges(value) {
                    declarations.padding = values.map(Some);
                    declarations.padding_order = [declaration_order; 4];
                    declarations.box_model_importance.padding = [important; 4];
                }
            }
            "padding-block" => {
                set_logical_border_pair(
                    &mut declarations.logical_box_model.padding,
                    &mut declarations.logical_box_model.padding_order,
                    &mut declarations.logical_box_model.padding_important,
                    [LOGICAL_BORDER_BLOCK_START, LOGICAL_BORDER_BLOCK_END],
                    parse_local_box_edge_pair(value),
                    declaration_order,
                    important,
                );
            }
            "padding-block-start" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.padding,
                    &mut declarations.logical_box_model.padding_order,
                    &mut declarations.logical_box_model.padding_important,
                    LOGICAL_BORDER_BLOCK_START,
                    parse_local_padding_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "padding-block-end" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.padding,
                    &mut declarations.logical_box_model.padding_order,
                    &mut declarations.logical_box_model.padding_important,
                    LOGICAL_BORDER_BLOCK_END,
                    parse_local_padding_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "padding-inline" => {
                set_logical_border_pair(
                    &mut declarations.logical_box_model.padding,
                    &mut declarations.logical_box_model.padding_order,
                    &mut declarations.logical_box_model.padding_important,
                    [LOGICAL_BORDER_INLINE_START, LOGICAL_BORDER_INLINE_END],
                    parse_local_box_edge_pair(value),
                    declaration_order,
                    important,
                );
            }
            "padding-inline-start" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.padding,
                    &mut declarations.logical_box_model.padding_order,
                    &mut declarations.logical_box_model.padding_important,
                    LOGICAL_BORDER_INLINE_START,
                    parse_local_padding_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "padding-inline-end" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.padding,
                    &mut declarations.logical_box_model.padding_order,
                    &mut declarations.logical_box_model.padding_important,
                    LOGICAL_BORDER_INLINE_END,
                    parse_local_padding_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "margin" => {
                if let Some(values) = parse_local_margin_edges(value) {
                    declarations.margin = values.map(Some);
                    declarations.margin_order = [declaration_order; 4];
                    declarations.box_model_importance.margin = [important; 4];
                }
            }
            "margin-block" => {
                set_logical_border_pair(
                    &mut declarations.logical_box_model.margin,
                    &mut declarations.logical_box_model.margin_order,
                    &mut declarations.logical_box_model.margin_important,
                    [LOGICAL_BORDER_BLOCK_START, LOGICAL_BORDER_BLOCK_END],
                    parse_local_margin_edge_pair(value),
                    declaration_order,
                    important,
                );
            }
            "margin-block-start" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.margin,
                    &mut declarations.logical_box_model.margin_order,
                    &mut declarations.logical_box_model.margin_important,
                    LOGICAL_BORDER_BLOCK_START,
                    parse_local_margin_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "margin-block-end" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.margin,
                    &mut declarations.logical_box_model.margin_order,
                    &mut declarations.logical_box_model.margin_important,
                    LOGICAL_BORDER_BLOCK_END,
                    parse_local_margin_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "margin-inline" => {
                set_logical_border_pair(
                    &mut declarations.logical_box_model.margin,
                    &mut declarations.logical_box_model.margin_order,
                    &mut declarations.logical_box_model.margin_important,
                    [LOGICAL_BORDER_INLINE_START, LOGICAL_BORDER_INLINE_END],
                    parse_local_margin_edge_pair(value),
                    declaration_order,
                    important,
                );
            }
            "margin-inline-start" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.margin,
                    &mut declarations.logical_box_model.margin_order,
                    &mut declarations.logical_box_model.margin_important,
                    LOGICAL_BORDER_INLINE_START,
                    parse_local_margin_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "margin-inline-end" => {
                set_logical_border_side(
                    &mut declarations.logical_box_model.margin,
                    &mut declarations.logical_box_model.margin_order,
                    &mut declarations.logical_box_model.margin_important,
                    LOGICAL_BORDER_INLINE_END,
                    parse_local_margin_declaration(value),
                    declaration_order,
                    important,
                );
            }
            "padding-top" => {
                if let Some(value) = parse_local_physical_padding_declaration(value) {
                    declarations.padding[0] = Some(value);
                    declarations.padding_order[0] = declaration_order;
                    declarations.box_model_importance.padding[0] = important;
                }
            }
            "padding-right" => {
                if let Some(value) = parse_local_physical_padding_declaration(value) {
                    declarations.padding[1] = Some(value);
                    declarations.padding_order[1] = declaration_order;
                    declarations.box_model_importance.padding[1] = important;
                }
            }
            "padding-bottom" => {
                if let Some(value) = parse_local_physical_padding_declaration(value) {
                    declarations.padding[2] = Some(value);
                    declarations.padding_order[2] = declaration_order;
                    declarations.box_model_importance.padding[2] = important;
                }
            }
            "padding-left" => {
                if let Some(value) = parse_local_physical_padding_declaration(value) {
                    declarations.padding[3] = Some(value);
                    declarations.padding_order[3] = declaration_order;
                    declarations.box_model_importance.padding[3] = important;
                }
            }
            "margin-top" => {
                if let Some(value) = parse_local_physical_margin_declaration(value) {
                    declarations.margin[0] = Some(value);
                    declarations.margin_order[0] = declaration_order;
                    declarations.box_model_importance.margin[0] = important;
                }
            }
            "margin-right" => {
                if let Some(value) = parse_local_physical_margin_declaration(value) {
                    declarations.margin[1] = Some(value);
                    declarations.margin_order[1] = declaration_order;
                    declarations.box_model_importance.margin[1] = important;
                }
            }
            "margin-bottom" => {
                if let Some(value) = parse_local_physical_margin_declaration(value) {
                    declarations.margin[2] = Some(value);
                    declarations.margin_order[2] = declaration_order;
                    declarations.box_model_importance.margin[2] = important;
                }
            }
            "margin-left" => {
                if let Some(value) = parse_local_physical_margin_declaration(value) {
                    declarations.margin[3] = Some(value);
                    declarations.margin_order[3] = declaration_order;
                    declarations.box_model_importance.margin[3] = important;
                }
            }
            "box-sizing" => {
                if let Some(value) = parse_local_box_sizing_declaration(value) {
                    declarations.box_sizing = Some(value);
                    declarations.box_model_importance.box_sizing = important;
                }
            }
            "color" => {
                if let Some(parsed) = parse_local_color_declaration(value) {
                    declarations.color = Some(parsed);
                    declarations.color_important = important;
                }
            }
            "overflow" => {
                if let Some(parsed) = parse_overflow_declaration(value) {
                    declarations.overflow = Some(parsed);
                    declarations.overflow_x = Some(parsed);
                    declarations.overflow_y = Some(parsed);
                    declarations.overflow_importance.shorthand = important;
                    declarations.overflow_importance.x = important;
                    declarations.overflow_importance.y = important;
                }
            }
            "overflow-x" => {
                if let Some(parsed) = parse_overflow_declaration(value) {
                    declarations.overflow_x = Some(parsed);
                    declarations.overflow_importance.x = important;
                }
            }
            "overflow-y" => {
                if let Some(parsed) = parse_overflow_declaration(value) {
                    declarations.overflow_y = Some(parsed);
                    declarations.overflow_importance.y = important;
                }
            }
            _ => {}
        }
    }
    declarations
}
