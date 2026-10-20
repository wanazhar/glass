use super::*;

pub(in crate::native_engine::css) fn parse_source(
    source: &str,
    output: NativeCssParseOutput<'_>,
) -> Result<(), NativeEngineError> {
    let source = strip_comments(source);
    let end = source.len();
    let mut context = NativeCssParseContext {
        source: &source,
        rules: output.rules,
        next_order: output.next_order,
        diagnostic_source: output.diagnostic_source,
        diagnostics: output.diagnostics,
        layers: output.layers,
        background_image_sources: output.background_image_sources,
        font_face_rules: output.font_face_rules,
        palette_values: output.palette_values,
        namespaces: BTreeMap::new(),
    };
    parse_source_block(&mut context, 0, end, None, None, None)
}

pub(in crate::native_engine::css) struct NativeCssParseOutput<'a> {
    pub(in crate::native_engine::css) rules: &'a mut Vec<NativeStyleRule>,
    pub(in crate::native_engine::css) next_order: &'a mut usize,
    pub(in crate::native_engine::css) diagnostic_source: NativeDiagnosticSource,
    pub(in crate::native_engine::css) diagnostics: &'a mut NativeDiagnosticSink,
    pub(in crate::native_engine::css) layers: &'a mut Vec<String>,
    pub(in crate::native_engine::css) background_image_sources: &'a mut BTreeMap<u32, String>,
    pub(in crate::native_engine::css) font_face_rules: &'a mut Vec<NativeFontFaceRule>,
    pub(in crate::native_engine::css) palette_values: &'a mut Vec<NativeFontPaletteValuesRule>,
}

struct NativeCssParseContext<'a> {
    source: &'a str,
    rules: &'a mut Vec<NativeStyleRule>,
    next_order: &'a mut usize,
    diagnostic_source: NativeDiagnosticSource,
    diagnostics: &'a mut NativeDiagnosticSink,
    layers: &'a mut Vec<String>,
    background_image_sources: &'a mut BTreeMap<u32, String>,
    font_face_rules: &'a mut Vec<NativeFontFaceRule>,
    palette_values: &'a mut Vec<NativeFontPaletteValuesRule>,
    namespaces: BTreeMap<String, NativeSelectorNamespace>,
}

fn parse_source_block(
    context: &mut NativeCssParseContext<'_>,
    start: usize,
    end: usize,
    current_layer: Option<usize>,
    current_media: Option<&str>,
    current_supports: Option<&str>,
) -> Result<(), NativeEngineError> {
    let mut cursor = start;
    while cursor < end {
        let remaining = &context.source[cursor..end];
        let Some(open_relative) = remaining.find('{') else {
            let trailing = remaining.trim();
            if !trailing.is_empty() {
                if parse_layer_header(trailing).is_some() {
                    context.diagnostics.push(
                        NativeDiagnosticCode::UnsupportedCssValue,
                        context.diagnostic_source,
                        cursor,
                        "layer-statement",
                    );
                } else {
                    context.diagnostics.push(
                        NativeDiagnosticCode::MalformedCss,
                        context.diagnostic_source,
                        cursor,
                        "missing-rule",
                    );
                }
            }
            return Ok(());
        };
        if let Some(semicolon_relative) = remaining[..open_relative].find(';') {
            let semicolon = cursor + semicolon_relative;
            if parse_layer_header(&context.source[cursor..semicolon]).is_some() {
                context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    cursor,
                    "layer-statement",
                );
                cursor = semicolon + 1;
                continue;
            }
            if let Some(namespace) = parse_namespace_statement(&context.source[cursor..semicolon]) {
                match namespace {
                    Ok((prefix, namespace)) => {
                        context.namespaces.insert(prefix, namespace);
                    }
                    Err(detail) => context.diagnostics.push(
                        NativeDiagnosticCode::UnsupportedCssValue,
                        context.diagnostic_source,
                        cursor,
                        detail,
                    ),
                }
                cursor = semicolon + 1;
                continue;
            }
        }
        let open = cursor + open_relative;
        let Some(close) = find_matching_brace(context.source, open, end) else {
            context.diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                context.diagnostic_source,
                open,
                "unclosed-rule",
            );
            return Ok(());
        };
        let header = &context.source[cursor..open];
        if is_font_face_header(header) {
            parse_font_face_rule(context, cursor, open, close)?;
        } else if let Some(palette_header) = parse_font_palette_values_header(header) {
            match palette_header {
                Ok(name) => parse_font_palette_values_rule(context, name, open, close)?,
                Err(detail) => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    cursor,
                    detail,
                ),
            }
        } else if let Some(media_header) = parse_media_header(header) {
            if current_media.is_some() {
                context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    cursor,
                    "nested-media",
                );
            } else {
                parse_source_block(
                    context,
                    open + 1,
                    close,
                    current_layer,
                    Some(media_header),
                    current_supports,
                )?;
            }
        } else if let Some(supports_header) = parse_supports_header(header) {
            if current_supports.is_some() {
                context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    cursor,
                    "nested-supports",
                );
            } else {
                parse_source_block(
                    context,
                    open + 1,
                    close,
                    current_layer,
                    current_media,
                    Some(supports_header),
                )?;
            }
        } else if let Some(layer_header) = parse_layer_header(header) {
            if current_layer.is_some() {
                context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    cursor,
                    "nested-layer",
                );
            } else {
                match layer_header {
                    Ok(name) => {
                        let Some(layer) = register_named_layer(context.layers, &name) else {
                            context.diagnostics.push(
                                NativeDiagnosticCode::UnsupportedCssValue,
                                context.diagnostic_source,
                                cursor,
                                "too-many-layers",
                            );
                            cursor = close + 1;
                            continue;
                        };
                        parse_source_block(
                            context,
                            open + 1,
                            close,
                            Some(layer),
                            current_media,
                            current_supports,
                        )?;
                    }
                    Err(detail) => context.diagnostics.push(
                        NativeDiagnosticCode::UnsupportedCssValue,
                        context.diagnostic_source,
                        cursor,
                        detail,
                    ),
                }
            }
        } else {
            parse_style_rule(
                context,
                cursor,
                open,
                close,
                current_layer,
                current_media,
                current_supports,
            )?;
        }
        cursor = close + 1;
    }
    Ok(())
}

pub(in crate::native_engine::css) fn parse_namespace_statement(
    source: &str,
) -> Option<Result<(String, NativeSelectorNamespace), &'static str>> {
    let source = source.trim();
    const KEYWORD: &str = "@namespace";
    if source.len() <= KEYWORD.len()
        || !source.as_bytes()[..KEYWORD.len()].eq_ignore_ascii_case(KEYWORD.as_bytes())
        || !source.as_bytes()[KEYWORD.len()].is_ascii_whitespace()
    {
        return None;
    }
    let remainder = source[KEYWORD.len()..].trim();
    let (prefix, uri_source) = if remainder
        .as_bytes()
        .first()
        .is_some_and(|byte| *byte == b'\'' || *byte == b'"')
        || remainder.len() >= 4 && remainder.as_bytes()[..4].eq_ignore_ascii_case(b"url(")
    {
        (String::new(), remainder)
    } else {
        let Some((prefix, next)) = read_identifier(remainder, 0) else {
            return Some(Err("namespace-prefix"));
        };
        if !remainder
            .as_bytes()
            .get(next)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            return Some(Err("namespace-prefix"));
        }
        (prefix.to_ascii_lowercase(), remainder[next..].trim())
    };
    let Some(uri) = parse_namespace_uri(uri_source) else {
        return Some(Err("namespace-uri"));
    };
    let Some(namespace) = selector_namespace_for_uri(uri) else {
        return Some(Err("namespace-uri"));
    };
    Some(Ok((prefix, namespace)))
}

fn parse_namespace_uri(source: &str) -> Option<&str> {
    let source = source.trim();
    let value = if source.len() >= 4
        && source.as_bytes()[..4].eq_ignore_ascii_case(b"url(")
        && source.ends_with(')')
    {
        source[4..source.len().saturating_sub(1)].trim()
    } else {
        source
    };
    if value.len() >= 2
        && matches!(
            (value.as_bytes().first(), value.as_bytes().last()),
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"'))
        )
    {
        return (!value[1..value.len().saturating_sub(1)].is_empty())
            .then_some(&value[1..value.len().saturating_sub(1)]);
    }
    (!value.is_empty() && !value.chars().any(char::is_whitespace)).then_some(value)
}

fn selector_namespace_for_uri(source: &str) -> Option<NativeSelectorNamespace> {
    match source {
        HTML_NAMESPACE_URI => Some(NativeSelectorNamespace::Html),
        SVG_NAMESPACE_URI => Some(NativeSelectorNamespace::Svg),
        MATHML_NAMESPACE_URI => Some(NativeSelectorNamespace::MathMl),
        XML_NAMESPACE_URI => Some(NativeSelectorNamespace::Xml),
        XMLNS_NAMESPACE_URI => Some(NativeSelectorNamespace::Xmlns),
        XLINK_NAMESPACE_URI => Some(NativeSelectorNamespace::Xlink),
        _ => None,
    }
}

fn parse_media_header(source: &str) -> Option<&str> {
    let source = source.trim();
    const KEYWORD: &str = "@media";
    if source.len() <= KEYWORD.len()
        || !source.as_bytes()[..KEYWORD.len()].eq_ignore_ascii_case(KEYWORD.as_bytes())
        || !source.as_bytes()[KEYWORD.len()].is_ascii_whitespace()
    {
        return None;
    }
    let condition = source[KEYWORD.len()..].trim();
    (!condition.is_empty()).then_some(condition)
}

fn parse_supports_header(source: &str) -> Option<&str> {
    let source = source.trim();
    const KEYWORD: &str = "@supports";
    if source.len() <= KEYWORD.len()
        || !source.as_bytes()[..KEYWORD.len()].eq_ignore_ascii_case(KEYWORD.as_bytes())
        || !source.as_bytes()[KEYWORD.len()].is_ascii_whitespace()
    {
        return None;
    }
    let condition = source[KEYWORD.len()..].trim();
    (!condition.is_empty()).then_some(condition)
}

fn is_font_face_header(source: &str) -> bool {
    let source = source.trim();
    const KEYWORD: &str = "@font-face";
    source.len() == KEYWORD.len() && source.as_bytes().eq_ignore_ascii_case(KEYWORD.as_bytes())
}

fn parse_font_palette_values_header(
    source: &str,
) -> Option<Result<NativeFontPaletteName, &'static str>> {
    let source = source.trim();
    const KEYWORD: &str = "@font-palette-values";
    if source.len() <= KEYWORD.len()
        || !source.as_bytes()[..KEYWORD.len()].eq_ignore_ascii_case(KEYWORD.as_bytes())
        || !source.as_bytes()[KEYWORD.len()].is_ascii_whitespace()
    {
        return None;
    }
    let remainder = source[KEYWORD.len()..].trim();
    let Some((name, end)) = read_identifier(remainder, 0) else {
        return Some(Err("font-palette-values-name"));
    };
    if end != remainder.len() {
        return Some(Err("font-palette-values-header"));
    }
    Some(NativeFontPaletteName::parse(&name).ok_or("font-palette-values-name"))
}

pub(in crate::native_engine::css) fn parse_font_palette_overrides(
    value: &str,
) -> Option<[Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES]> {
    let tokens = split_css_value_tokens(value)?;
    let pair_count = tokens.len() / 2;
    if tokens.len() % 2 != 0 || pair_count > MAX_NATIVE_FONT_PALETTE_OVERRIDES {
        return None;
    }
    let mut overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES] =
        [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES];
    for pair in 0..pair_count {
        let palette_index = tokens[pair * 2]
            .parse::<u32>()
            .ok()
            .filter(|value| *value <= u32::from(u16::MAX))
            .and_then(|value| u16::try_from(value).ok())?;
        if overrides[..pair]
            .iter()
            .flatten()
            .any(|override_value| override_value.palette_index == palette_index)
        {
            return None;
        }
        overrides[pair] = Some(NativeFontPaletteOverride {
            palette_index,
            color: parse_color(tokens[pair * 2 + 1])?,
        });
    }
    Some(overrides)
}

fn parse_font_palette_values_rule(
    context: &mut NativeCssParseContext<'_>,
    name: NativeFontPaletteName,
    open: usize,
    close: usize,
) -> Result<(), NativeEngineError> {
    let source = &context.source[open + 1..close];
    let mut base_palette = 0;
    let mut overrides = [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES];
    let mut valid = true;
    for (declaration_offset, declaration) in split_css_declarations(source) {
        let offset = open.saturating_add(1).saturating_add(declaration_offset);
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some((property, value)) = declaration.split_once(':') else {
            context.diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                context.diagnostic_source,
                offset,
                "font-palette-values-missing-colon",
            );
            valid = false;
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let (value, _) = strip_important_suffix(value.trim());
        if value.is_empty() {
            context.diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                context.diagnostic_source,
                offset,
                "font-palette-values-empty-value",
            );
            valid = false;
            continue;
        }
        match property.as_str() {
            "base-palette" => {
                let mut values = value.split_ascii_whitespace();
                match (values.next(), values.next()) {
                    (Some(value), None) => match value.parse::<u32>() {
                        Ok(value) if value <= u32::from(u16::MAX) => {
                            base_palette = value as u16;
                        }
                        _ => {
                            context.diagnostics.push(
                                NativeDiagnosticCode::UnsupportedCssValue,
                                context.diagnostic_source,
                                offset,
                                "font-palette-values-base-palette",
                            );
                            valid = false;
                        }
                    },
                    _ => {
                        context.diagnostics.push(
                            NativeDiagnosticCode::UnsupportedCssValue,
                            context.diagnostic_source,
                            offset,
                            "font-palette-values-base-palette",
                        );
                        valid = false;
                    }
                }
            }
            "override-colors" => match parse_font_palette_overrides(value) {
                Some(parsed) => overrides = parsed,
                None => {
                    context.diagnostics.push(
                        NativeDiagnosticCode::UnsupportedCssValue,
                        context.diagnostic_source,
                        offset,
                        "font-palette-override-colors",
                    );
                    valid = false;
                }
            },
            _ => {
                context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssProperty,
                    context.diagnostic_source,
                    offset,
                    "font-palette-values-descriptor",
                );
                valid = false;
            }
        }
    }
    if !valid {
        return Ok(());
    }
    if context.palette_values.len() >= MAX_NATIVE_FONT_PALETTE_RULES {
        context.diagnostics.push(
            NativeDiagnosticCode::UnsupportedCssValue,
            context.diagnostic_source,
            open,
            "too-many-font-palette-values",
        );
        return Ok(());
    }
    context.palette_values.push(NativeFontPaletteValuesRule {
        name,
        base_palette,
        overrides,
    });
    Ok(())
}

fn parse_font_face_rule(
    context: &mut NativeCssParseContext<'_>,
    selector_start: usize,
    open: usize,
    close: usize,
) -> Result<(), NativeEngineError> {
    let source = &context.source[open + 1..close];
    let mut family = None;
    let mut font_sources = None;
    let mut weight = NativeFontWeightRange::default();
    let mut style = FontStyleValue::Normal;
    let mut stretch = NativeFontStretchRange::default();
    let mut unicode_ranges = Vec::new();
    let mut variation_settings = NativeFontVariationSettings::default();
    let mut feature_settings = NativeFontFeatureSettings::default();
    let mut size_adjust = DEFAULT_NATIVE_FONT_SIZE_ADJUST;
    let mut metric_overrides = NativeFontMetricOverrides::default();
    let mut font_display = NativeFontDisplay::default();
    for (declaration_offset, declaration) in split_css_declarations(source) {
        let offset = open.saturating_add(1).saturating_add(declaration_offset);
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some((property, value)) = declaration.split_once(':') else {
            context.diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                context.diagnostic_source,
                offset,
                "font-face-missing-colon",
            );
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let (value, _) = strip_important_suffix(value.trim());
        if value.is_empty() {
            context.diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                context.diagnostic_source,
                offset,
                "font-face-empty-value",
            );
            continue;
        }
        match property.as_str() {
            "font-family" => match parse_font_face_family(value) {
                Some(parsed) => family = Some(parsed),
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-family",
                ),
            },
            "src" => match parse_font_face_src(value) {
                Some(parsed) => font_sources = Some(parsed),
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-src",
                ),
            },
            "font-weight" => match parse_font_weight_range(value) {
                Some(parsed) => weight = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-weight",
                ),
            },
            "font-style" => match parse_font_style(value) {
                Some(parsed) => style = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-style",
                ),
            },
            "font-stretch" => match parse_font_stretch_range(value) {
                Some(parsed) => stretch = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-stretch",
                ),
            },
            "unicode-range" => match parse_font_face_unicode_range(value) {
                Some(parsed) => unicode_ranges = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-unicode-range",
                ),
            },
            "font-variation-settings" => match parse_font_variation_settings(value) {
                Some(parsed) => variation_settings = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-variation-settings",
                ),
            },
            "font-feature-settings" => match parse_font_feature_settings(value) {
                Some(parsed) => feature_settings = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-feature-settings",
                ),
            },
            "size-adjust" => match parse_font_size_adjust(value) {
                Some(parsed) => size_adjust = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-size-adjust",
                ),
            },
            "ascent-override" => match parse_font_metric_override(value) {
                Some(parsed) => metric_overrides.ascent = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-ascent-override",
                ),
            },
            "descent-override" => match parse_font_metric_override(value) {
                Some(parsed) => metric_overrides.descent = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-descent-override",
                ),
            },
            "line-gap-override" => match parse_font_metric_override(value) {
                Some(parsed) => metric_overrides.line_gap = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-line-gap-override",
                ),
            },
            "font-display" => match parse_font_display(value) {
                Some(parsed) => font_display = parsed,
                None => context.diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssValue,
                    context.diagnostic_source,
                    offset,
                    "font-face-display",
                ),
            },
            _ => context.diagnostics.push(
                NativeDiagnosticCode::UnsupportedCssProperty,
                context.diagnostic_source,
                offset,
                &property,
            ),
        }
    }
    let Some(family) = family else {
        context.diagnostics.push(
            NativeDiagnosticCode::UnsupportedCssValue,
            context.diagnostic_source,
            selector_start,
            "font-face-family",
        );
        return Ok(());
    };
    let Some(sources) = font_sources else {
        context.diagnostics.push(
            NativeDiagnosticCode::UnsupportedCssValue,
            context.diagnostic_source,
            selector_start,
            "font-face-src",
        );
        return Ok(());
    };
    if context.font_face_rules.len() >= MAX_NATIVE_FONT_FACE_RULES {
        return Err(NativeEngineError::limit(
            "CSS @font-face rules",
            MAX_NATIVE_FONT_FACE_RULES,
            context.font_face_rules.len().saturating_add(1),
        ));
    }
    context.font_face_rules.push(NativeFontFaceRule {
        family_key: font_family_hash(&family),
        family,
        sources,
        weight,
        style,
        stretch,
        unicode_ranges,
        variation_settings,
        feature_settings,
        size_adjust,
        metric_overrides,
        font_display,
    });
    Ok(())
}

fn parse_style_rule(
    context: &mut NativeCssParseContext<'_>,
    selector_start: usize,
    open: usize,
    close: usize,
    layer: Option<usize>,
    media: Option<&str>,
    supports: Option<&str>,
) -> Result<(), NativeEngineError> {
    let source = context.source;
    let declarations = parse_declarations_with_diagnostics(
        &source[open + 1..close],
        context.diagnostic_source,
        open.saturating_add(1),
        context.diagnostics,
    );
    let custom_properties = parse_custom_property_declarations(&source[open + 1..close]);
    collect_background_image_sources(&source[open + 1..close], context.background_image_sources);
    let has_supported_declaration = declarations.display.is_some()
        || declarations.position.is_some()
        || declarations.z_index.is_some()
        || declarations.pointer_events.is_some()
        || declarations.top.is_some()
        || declarations.right.is_some()
        || declarations.bottom.is_some()
        || declarations.left.is_some()
        || declarations.grid_template_columns.is_some()
        || declarations.grid_template_rows.is_some()
        || declarations.visibility.is_some()
        || declarations.opacity.is_some()
        || declarations.white_space.is_some()
        || declarations.text_align.is_some()
        || declarations.text_align_last.is_some()
        || declarations.text_justify.is_some()
        || declarations.justify_content.is_some()
        || declarations.align_items.is_some()
        || declarations.align_self.is_some()
        || declarations.align_content.is_some()
        || declarations.flex_direction.is_some()
        || declarations.direction.is_some()
        || declarations.flex_wrap.is_some()
        || declarations.order.is_some()
        || declarations.flex_grow.is_some()
        || declarations.flex_shrink.is_some()
        || declarations.flex_basis.is_some()
        || declarations.text_decoration.is_some()
        || declarations.text_decoration_style.is_some()
        || declarations.text_decoration_skip_ink.is_some()
        || declarations.text_decoration_skip_spaces.is_some()
        || declarations.text_decoration_thickness.is_some()
        || declarations.text_underline_offset.is_some()
        || declarations.text_decoration_color.is_some()
        || declarations.text_transform.is_some()
        || declarations.font_variant_ligatures.is_some()
        || declarations.font_variant_caps.is_some()
        || declarations.font_variant_position.is_some()
        || declarations.font_variant_alternates.is_some()
        || declarations.font_language_override.is_some()
        || declarations.font_variation_settings.is_some()
        || declarations.font_variant_east_asian.is_some()
        || declarations.font_variant_numeric.is_some()
        || declarations.font_feature_settings.is_some()
        || declarations.font_kerning.is_some()
        || declarations.font_optical_sizing.is_some()
        || declarations.font_palette.is_some()
        || declarations.font_weight.is_some()
        || declarations.font_style.is_some()
        || declarations.font_stretch.is_some()
        || declarations.font_family.is_some()
        || declarations.font_size.is_some()
        || declarations.word_break.is_some()
        || declarations.text_overflow.is_some()
        || declarations.vertical_align.is_some()
        || declarations.text_indent.is_some()
        || declarations.word_spacing.is_some()
        || declarations.letter_spacing.is_some()
        || declarations.gap.is_some()
        || declarations.row_gap.is_some()
        || declarations.column_gap.is_some()
        || declarations.width.is_some()
        || declarations.height.is_some()
        || declarations.min_width.is_some()
        || declarations.max_width.is_some()
        || declarations.min_height.is_some()
        || declarations.max_height.is_some()
        || declarations.line_height.is_some()
        || declarations.background_color.is_some()
        || declarations.background_image.is_some()
        || declarations.background_repeat.is_some()
        || declarations.background_position.is_some()
        || declarations.background_size.is_some()
        || declarations.border.iter().any(Option::is_some)
        || declarations.logical_border.has_any()
        || declarations.border_width.iter().any(Option::is_some)
        || declarations.border_style.iter().any(Option::is_some)
        || declarations.border_color.iter().any(Option::is_some)
        || declarations.border_radius.is_some()
        || declarations
            .border_radius_corners
            .iter()
            .any(Option::is_some)
        || declarations.logical_border_radius.has_any()
        || declarations.logical_box_model.has_any()
        || declarations.padding.iter().any(Option::is_some)
        || declarations.margin.iter().any(Option::is_some)
        || declarations.box_sizing.is_some()
        || declarations.color.is_some()
        || declarations.overflow.is_some()
        || declarations.overflow_x.is_some()
        || declarations.overflow_y.is_some()
        || !custom_properties.is_empty();
    let selector_source = &source[selector_start..open];
    let selector_parts = split_selector_list_with_offsets(selector_source)
        .unwrap_or_else(|| vec![(0, selector_source)]);
    for (selector_relative_offset, selector_text) in selector_parts {
        let Some(mut selector) = parse_selector_with_namespaces(selector_text, &context.namespaces)
        else {
            context.diagnostics.push(
                NativeDiagnosticCode::UnsupportedCssSelector,
                context.diagnostic_source,
                selector_start.saturating_add(selector_relative_offset),
                selector_diagnostic_detail(selector_text),
            );
            continue;
        };
        if has_supported_declaration {
            if context.rules.len() >= MAX_NATIVE_STYLE_RULES {
                return Err(NativeEngineError::limit(
                    "CSS style rules",
                    MAX_NATIVE_STYLE_RULES,
                    context.rules.len().saturating_add(1),
                ));
            }
            selector.specificity = encode_cascade_specificity(selector.specificity, layer);
            context.rules.push(NativeStyleRule {
                selector,
                declarations,
                custom_properties: custom_properties.clone(),
                media: media.map(str::to_owned),
                supports: supports.map(str::to_owned),
                order: *context.next_order,
            });
            *context.next_order = context.next_order.saturating_add(1);
        }
    }
    Ok(())
}
fn find_matching_brace(source: &str, open: usize, end: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in source[open..end].bytes().enumerate() {
        match byte {
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_layer_header(source: &str) -> Option<Result<String, &'static str>> {
    let source = source.trim();
    let keyword = "@layer";
    if source.len() < keyword.len()
        || !source
            .as_bytes()
            .get(..keyword.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(keyword.as_bytes()))
        || !source
            .as_bytes()
            .get(keyword.len())
            .is_none_or(|byte| byte.is_ascii_whitespace())
    {
        return None;
    }
    let rest = source[keyword.len()..].trim();
    if rest.is_empty() {
        return Some(Err("layer-name"));
    }
    let Some((name, next)) = read_identifier(rest, 0) else {
        return Some(Err("layer-name"));
    };
    if next != rest.len() {
        Some(Err("layer-prelude"))
    } else {
        Some(Ok(name))
    }
}

fn register_named_layer(layers: &mut Vec<String>, name: &str) -> Option<usize> {
    if let Some(index) = layers.iter().position(|layer| layer == name) {
        return Some(index);
    }
    if layers.len() >= MAX_NATIVE_NAMED_CASCADE_LAYERS {
        return None;
    }
    let index = layers.len();
    layers.push(name.to_owned());
    Some(index)
}
