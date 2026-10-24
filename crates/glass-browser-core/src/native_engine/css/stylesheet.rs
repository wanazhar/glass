use super::*;

impl NativeStylesheet {
    /// Whether a node can receive a background-image declaration from a
    /// matching stylesheet rule. This is a conservative candidate check: the
    /// normal computed-style cascade still chooses the final value.
    pub(crate) fn has_background_image_rule_for_node(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };
        self.background_image_rule_index
            .candidates(node)
            .into_iter()
            .any(|rule_index| {
                let rule = &self.rules[rule_index];
                !rule
                    .media
                    .as_deref()
                    .is_some_and(|media| !css_media_list_matches(media, viewport))
                    && !rule
                        .supports
                        .as_deref()
                        .is_some_and(|supports| !css_supports_condition_matches(supports))
                    && rule.selector.matches_in_document(document, node_id)
            })
    }

    #[cfg(test)]
    pub(crate) fn from_sources(
        sources: impl IntoIterator<Item = String>,
    ) -> Result<Self, NativeEngineError> {
        let mut diagnostics = NativeDiagnosticSink::default();
        Self::from_sources_with_diagnostics(sources, &mut diagnostics)
    }

    pub(crate) fn from_sources_with_diagnostics(
        sources: impl IntoIterator<Item = String>,
        diagnostics: &mut NativeDiagnosticSink,
    ) -> Result<Self, NativeEngineError> {
        let mut stylesheet = Self::default();
        let mut order = 0;
        let mut layers = Vec::new();
        for (index, source) in sources.into_iter().enumerate() {
            parse_source(
                &source,
                NativeCssParseOutput {
                    rules: &mut stylesheet.rules,
                    next_order: &mut order,
                    diagnostic_source: NativeDiagnosticSource::Stylesheet { index },
                    diagnostics,
                    layers: &mut layers,
                    background_image_sources: &mut stylesheet.background_image_sources,
                    font_face_rules: &mut stylesheet.font_face_rules,
                    palette_values: &mut stylesheet.font_palette_values,
                },
            )?;
        }
        stylesheet.rule_index = NativeStyleRuleIndex::build(&stylesheet.rules);
        stylesheet.background_image_rule_index = NativeStyleRuleIndex::build_selected(
            &stylesheet.rules,
            stylesheet
                .rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| rule.declarations.background_image.is_some())
                .map(|(index, _)| index),
        );
        Ok(stylesheet)
    }

    pub(crate) fn background_image_sources(&self) -> &BTreeMap<u32, String> {
        &self.background_image_sources
    }

    pub(crate) fn font_face_rules(&self) -> &[NativeFontFaceRule] {
        &self.font_face_rules
    }

    #[cfg(test)]
    pub(crate) fn font_palette_values(&self) -> &[NativeFontPaletteValuesRule] {
        &self.font_palette_values
    }

    pub(crate) fn resolve_font_palette(&self, palette: NativeFontPalette) -> NativeFontPalette {
        match palette {
            NativeFontPalette::Named(name) => self
                .font_palette_values
                .iter()
                .rev()
                .find(|rule| rule.name == name)
                .map_or(NativeFontPalette::Normal, |rule| {
                    NativeFontPalette::Base(rule.base_palette)
                }),
            other => other,
        }
    }

    pub(crate) fn resolve_font_palette_overrides(
        &self,
        palette: NativeFontPalette,
    ) -> [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES] {
        match palette {
            NativeFontPalette::Named(name) => self
                .font_palette_values
                .iter()
                .rev()
                .find(|rule| rule.name == name)
                .map_or([None; MAX_NATIVE_FONT_PALETTE_OVERRIDES], |rule| {
                    rule.overrides
                }),
            _ => [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
        }
    }

    fn custom_properties_for_node_with_matcher(
        &self,
        node: &NativeNode,
        viewport: Viewport,
        matches: impl Fn(&NativeSelector) -> bool,
    ) -> BTreeMap<u64, String> {
        let mut candidates = BTreeMap::new();
        for rule_index in self.rule_index.candidates(node) {
            let rule = &self.rules[rule_index];
            if rule
                .media
                .as_deref()
                .is_some_and(|media| !css_media_list_matches(media, viewport))
                || rule
                    .supports
                    .as_deref()
                    .is_some_and(|supports| !css_supports_condition_matches(supports))
                || !matches(&rule.selector)
            {
                continue;
            }
            apply_custom_property_declarations(
                &mut candidates,
                &rule.custom_properties,
                rule.selector.specificity,
                rule.order,
                false,
            );
        }
        if node.inline_style_allowed()
            && let Some(inline_style) = node.attribute("style")
        {
            apply_custom_property_declarations(
                &mut candidates,
                &parse_custom_property_declarations(inline_style),
                u16::MAX,
                usize::MAX,
                true,
            );
        }
        candidates
            .into_iter()
            .map(|(name_hash, candidate)| (name_hash, candidate.value))
            .collect()
    }

    #[cfg(test)]
    fn custom_properties_for_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> BTreeMap<u64, String> {
        let mut ancestry = Vec::new();
        let mut current = Some(node_id);
        while let Some(id) = current {
            ancestry.push(id);
            current = document.node(id).and_then(NativeNode::parent);
        }
        let mut values = BTreeMap::new();
        for id in ancestry.into_iter().rev() {
            let Some(node) = document.node(id) else {
                continue;
            };
            values.extend(self.custom_properties_for_node_with_matcher(
                node,
                viewport,
                |selector| selector.matches_in_document(document, id),
            ));
        }
        values
    }

    pub(crate) fn custom_properties_for_node_in_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        viewport: Viewport,
    ) -> BTreeMap<u64, String> {
        let Some(node) = document.node(node_id) else {
            return BTreeMap::new();
        };
        self.custom_properties_for_node_with_matcher(node, viewport, |selector| {
            selector.matches_in_document(document, node_id)
        })
    }

    #[cfg(test)]
    pub(crate) fn computed_for(&self, node: &NativeNode) -> NativeComputedStyle {
        self.computed_for_with_viewport(node, Viewport::default())
    }

    #[cfg(test)]
    pub(crate) fn computed_for_with_viewport(
        &self,
        node: &NativeNode,
        viewport: Viewport,
    ) -> NativeComputedStyle {
        let custom_properties =
            self.custom_properties_for_node_with_matcher(node, viewport, |selector| {
                selector.matches(node)
            });
        self.computed_for_with_matcher_and_custom_properties(
            node,
            NativeInheritedStyle::default(),
            &custom_properties,
            viewport,
            |selector| selector.matches(node),
        )
    }
    #[cfg(test)]
    pub(crate) fn computed_for_in_document(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        inherited_color: Option<NativeColor>,
    ) -> NativeComputedStyle {
        self.computed_for_in_document_with_inheritance(
            document,
            node_id,
            NativeInheritedStyle {
                color: inherited_color,
                text_decoration_color: inherited_color.unwrap_or(NativeColor::BLACK),
                ..NativeInheritedStyle::default()
            },
            Viewport::default(),
        )
    }

    #[cfg(test)]
    pub(crate) fn computed_for_in_document_with_inheritance(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        inherited: NativeInheritedStyle,
        viewport: Viewport,
    ) -> NativeComputedStyle {
        let custom_properties = self.custom_properties_for_document(document, node_id, viewport);
        self.computed_for_in_document_with_inheritance_and_custom_properties(
            document,
            node_id,
            inherited,
            &custom_properties,
            viewport,
        )
    }

    pub(crate) fn computed_for_in_document_with_inheritance_and_custom_properties(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        inherited: NativeInheritedStyle,
        custom_properties: &BTreeMap<u64, String>,
        viewport: Viewport,
    ) -> NativeComputedStyle {
        let Some(node) = document.node(node_id) else {
            return NativeComputedStyle::default();
        };
        self.computed_for_with_matcher_and_custom_properties(
            node,
            inherited,
            custom_properties,
            viewport,
            |selector| selector.matches_in_document(document, node_id),
        )
    }

    #[cfg(test)]
    // The cascade locals are mutable references into heap-owned scratch. The
    // explicit reborrows below keep each helper's mutable borrow short-lived
    // while preserving the stack-safe allocation boundary.
    #[allow(clippy::explicit_auto_deref, clippy::needless_borrow)]
    pub(super) fn computed_for_with_matcher(
        &self,
        node: &NativeNode,
        inherited: NativeInheritedStyle,
        matches: impl Fn(&NativeSelector) -> bool,
    ) -> NativeComputedStyle {
        let custom_properties = BTreeMap::new();
        self.computed_for_with_matcher_and_custom_properties(
            node,
            inherited,
            &custom_properties,
            Viewport::default(),
            matches,
        )
    }

    #[allow(clippy::explicit_auto_deref, clippy::needless_borrow)]
    pub(super) fn computed_for_with_matcher_and_custom_properties(
        &self,
        node: &NativeNode,
        inherited: NativeInheritedStyle,
        custom_properties: &BTreeMap<u64, String>,
        viewport: Viewport,
        matches: impl Fn(&NativeSelector) -> bool,
    ) -> NativeComputedStyle {
        let mut scratch = NativeCascadeScratch::new_boxed();
        let mut display = &mut scratch.display;
        let mut position = &mut scratch.position;
        let mut z_index = &mut scratch.z_index;
        let mut pointer_events = &mut scratch.pointer_events;
        let mut top = &mut scratch.top;
        let mut right = &mut scratch.right;
        let mut bottom = &mut scratch.bottom;
        let mut left = &mut scratch.left;
        let mut grid_template_columns = &mut scratch.grid_template_columns;
        let mut grid_template_rows = &mut scratch.grid_template_rows;
        let mut visibility = &mut scratch.visibility;
        let mut opacity = &mut scratch.opacity;
        let mut white_space = &mut scratch.white_space;
        let mut text_align = &mut scratch.text_align;
        let mut text_align_last = &mut scratch.text_align_last;
        let mut text_justify = &mut scratch.text_justify;
        let mut direction = &mut scratch.direction;
        let mut justify_content = &mut scratch.justify_content;
        let mut align_items = &mut scratch.align_items;
        let mut align_self = &mut scratch.align_self;
        let mut align_content = &mut scratch.align_content;
        let mut flex_direction = &mut scratch.flex_direction;
        let mut flex_wrap = &mut scratch.flex_wrap;
        let mut flex_item_order = &mut scratch.flex_item_order;
        let mut flex_grow = &mut scratch.flex_grow;
        let mut flex_shrink = &mut scratch.flex_shrink;
        let mut flex_basis = &mut scratch.flex_basis;
        let mut text_decoration = &mut scratch.text_decoration;
        let mut text_decoration_style = &mut scratch.text_decoration_style;
        let mut text_decoration_skip_ink = &mut scratch.text_decoration_skip_ink;
        let mut text_decoration_skip_spaces = &mut scratch.text_decoration_skip_spaces;
        let mut text_decoration_thickness = &mut scratch.text_decoration_thickness;
        let mut text_underline_offset = &mut scratch.text_underline_offset;
        let mut text_decoration_color = &mut scratch.text_decoration_color;
        let mut text_transform = &mut scratch.text_transform;
        let mut font_variant_ligatures = &mut scratch.font_variant_ligatures;
        let mut font_variant_caps = &mut scratch.font_variant_caps;
        let mut font_variant_position = &mut scratch.font_variant_position;
        let mut font_variant_alternates = &mut scratch.font_variant_alternates;
        let mut font_language_override = &mut scratch.font_language_override;
        let mut font_variation_settings = &mut scratch.font_variation_settings;
        let mut font_variant_east_asian = &mut scratch.font_variant_east_asian;
        let mut font_variant_numeric = &mut scratch.font_variant_numeric;
        let mut font_feature_settings = &mut scratch.font_feature_settings;
        let mut font_kerning = &mut scratch.font_kerning;
        let mut font_optical_sizing = &mut scratch.font_optical_sizing;
        let mut font_palette = &mut scratch.font_palette;
        let mut font_weight = &mut scratch.font_weight;
        let mut font_style = &mut scratch.font_style;
        let mut font_stretch = &mut scratch.font_stretch;
        let mut font_family = &mut scratch.font_family;
        let mut font_size = &mut scratch.font_size;
        let mut word_break = &mut scratch.word_break;
        let mut text_overflow = &mut scratch.text_overflow;
        let mut vertical_align = &mut scratch.vertical_align;
        let mut text_indent = &mut scratch.text_indent;
        let mut word_spacing = &mut scratch.word_spacing;
        let mut letter_spacing = &mut scratch.letter_spacing;
        let mut gap = &mut scratch.gap;
        let mut width = &mut scratch.width;
        let mut height = &mut scratch.height;
        let mut min_width = &mut scratch.min_width;
        let mut max_width = &mut scratch.max_width;
        let mut min_height = &mut scratch.min_height;
        let mut max_height = &mut scratch.max_height;
        let mut line_height = &mut scratch.line_height;
        let mut background_color = &mut scratch.background_color;
        let mut background_image = &mut scratch.background_image;
        let mut background_repeat = &mut scratch.background_repeat;
        let mut background_position = &mut scratch.background_position;
        let mut background_size = &mut scratch.background_size;
        let mut border = &mut scratch.border;
        let mut logical_border = &mut scratch.logical_border;
        let mut border_width = &mut scratch.border_width;
        let mut logical_border_width = &mut scratch.logical_border_width;
        let mut border_style = &mut scratch.border_style;
        let mut logical_border_style = &mut scratch.logical_border_style;
        let mut border_color = &mut scratch.border_color;
        let mut logical_border_color = &mut scratch.logical_border_color;
        let mut border_radius = &mut scratch.border_radius;
        let mut logical_border_radius = &mut scratch.logical_border_radius;
        let mut logical_padding = &mut scratch.logical_padding;
        let mut logical_margin = &mut scratch.logical_margin;
        let mut padding = &mut scratch.padding;
        let mut margin = &mut scratch.margin;
        let mut box_sizing = &mut scratch.box_sizing;
        let mut color = &mut scratch.color;
        let mut overflow_x = &mut scratch.overflow_x;
        let mut overflow_y = &mut scratch.overflow_y;
        for rule_index in self.rule_index.candidates(node) {
            let rule = &self.rules[rule_index];
            if rule
                .media
                .as_deref()
                .is_some_and(|media| !css_media_list_matches(media, viewport))
                || rule
                    .supports
                    .as_deref()
                    .is_some_and(|supports| !css_supports_condition_matches(supports))
                || !matches(&rule.selector)
            {
                continue;
            }
            apply_local_important_cascade_declaration(
                rule.declarations.display,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.display,
                &mut display,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.position,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.position,
                &mut position,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.z_index,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.z_index,
                &mut z_index,
            );
            apply_text_cascade_declaration(
                rule.declarations.pointer_events,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.pointer_events,
                &mut pointer_events,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.top,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.top,
                &mut top,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.right,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.right,
                &mut right,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.bottom,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.bottom,
                &mut bottom,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.left,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.left,
                &mut left,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.grid_template_columns,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.grid_template_columns_important,
                &mut grid_template_columns,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.grid_template_rows,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.grid_template_rows_important,
                &mut grid_template_rows,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.visibility,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.visibility,
                &mut visibility,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.opacity,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.local_importance.opacity,
                &mut opacity,
            );
            apply_text_cascade_declaration(
                rule.declarations.white_space,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.white_space,
                &mut white_space,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_align,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_align,
                &mut text_align,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_align_last,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_align_last,
                &mut text_align_last,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_justify,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_justify,
                &mut text_justify,
            );
            apply_text_cascade_declaration(
                rule.declarations.direction,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.direction,
                &mut direction,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_decoration,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_decoration,
                &mut text_decoration,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_decoration_style,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_decoration_style,
                &mut text_decoration_style,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_decoration_skip_ink,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_decoration_skip_ink,
                &mut text_decoration_skip_ink,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_decoration_skip_spaces,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations
                    .text_importance
                    .text_decoration_skip_spaces,
                &mut text_decoration_skip_spaces,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_decoration_thickness,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_decoration_thickness,
                &mut text_decoration_thickness,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_underline_offset,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_underline_offset,
                &mut text_underline_offset,
            );
            apply_paint_cascade_declaration(
                rule.declarations.text_decoration_color,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_decoration_color_important,
                &mut text_decoration_color,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_transform,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_transform,
                &mut text_transform,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variant_ligatures,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variant_ligatures,
                &mut font_variant_ligatures,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variant_caps,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variant_caps,
                &mut font_variant_caps,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variant_position,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variant_position,
                &mut font_variant_position,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variant_alternates,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variant_alternates,
                &mut font_variant_alternates,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_language_override,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_language_override,
                &mut font_language_override,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variation_settings,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variation_settings,
                &mut font_variation_settings,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variant_east_asian,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variant_east_asian,
                &mut font_variant_east_asian,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_variant_numeric,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_variant_numeric,
                &mut font_variant_numeric,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_feature_settings,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_feature_settings,
                &mut font_feature_settings,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_kerning,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_kerning,
                &mut font_kerning,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_optical_sizing,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_optical_sizing,
                &mut font_optical_sizing,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_palette,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_palette,
                &mut font_palette,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_weight,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_weight,
                &mut font_weight,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_style,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_style,
                &mut font_style,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_stretch,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_stretch,
                &mut font_stretch,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_family,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_family,
                &mut font_family,
            );
            apply_text_cascade_declaration(
                rule.declarations.font_size,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.font_size,
                &mut font_size,
            );
            apply_text_cascade_declaration(
                rule.declarations.word_break,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.word_break,
                &mut word_break,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_overflow,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_overflow,
                &mut text_overflow,
            );
            apply_text_cascade_declaration(
                rule.declarations.vertical_align,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.vertical_align,
                &mut vertical_align,
            );
            apply_text_cascade_declaration(
                rule.declarations.text_indent,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.text_indent,
                &mut text_indent,
            );
            apply_text_cascade_declaration(
                rule.declarations.word_spacing,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.word_spacing,
                &mut word_spacing,
            );
            apply_text_cascade_declaration(
                rule.declarations.letter_spacing,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.letter_spacing,
                &mut letter_spacing,
            );
            apply_gap_declarations(
                rule.declarations,
                rule.selector.specificity,
                rule.order,
                false,
                &mut gap,
            );
            apply_flex_cascade_declaration(
                rule.declarations.justify_content,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.justify_content,
                &mut justify_content,
            );
            apply_flex_cascade_declaration(
                rule.declarations.order,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.order,
                &mut flex_item_order,
            );
            apply_flex_cascade_declaration(
                rule.declarations.flex_grow,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.flex_grow,
                &mut flex_grow,
            );
            apply_flex_cascade_declaration(
                rule.declarations.flex_shrink,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.flex_shrink,
                &mut flex_shrink,
            );
            apply_flex_cascade_declaration(
                rule.declarations.flex_basis,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.flex_basis,
                &mut flex_basis,
            );
            apply_flex_cascade_declaration(
                rule.declarations.align_items,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.align_items,
                &mut align_items,
            );
            apply_flex_cascade_declaration(
                rule.declarations.align_self,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.align_self,
                &mut align_self,
            );
            apply_flex_cascade_declaration(
                rule.declarations.align_content,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.align_content,
                &mut align_content,
            );
            apply_flex_cascade_declaration(
                rule.declarations.flex_direction,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.flex_direction,
                &mut flex_direction,
            );
            apply_flex_cascade_declaration(
                rule.declarations.flex_wrap,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.flex_importance.flex_wrap,
                &mut flex_wrap,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.width,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.dimension_importance.width,
                &mut width,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.height,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.dimension_importance.height,
                &mut height,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.min_width,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.dimension_importance.min_width,
                &mut min_width,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.max_width,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.dimension_importance.max_width,
                &mut max_width,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.min_height,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.dimension_importance.min_height,
                &mut min_height,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.max_height,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.dimension_importance.max_height,
                &mut max_height,
            );
            apply_text_cascade_declaration(
                rule.declarations.line_height,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.text_importance.line_height,
                &mut line_height,
            );
            apply_paint_cascade_declaration(
                rule.declarations.background_color,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.background_color_important,
                &mut background_color,
            );
            apply_paint_cascade_declaration(
                rule.declarations.background_image,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.background_image_important,
                &mut background_image,
            );
            apply_paint_cascade_declaration(
                rule.declarations.background_repeat,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.background_repeat_important,
                &mut background_repeat,
            );
            apply_paint_cascade_declaration(
                rule.declarations.background_position,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.background_position_important,
                &mut background_position,
            );
            apply_paint_cascade_declaration(
                rule.declarations.background_size,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.background_size_important,
                &mut background_size,
            );
            apply_local_cascade_edges(
                &rule.declarations.border,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border,
            );
            apply_border_width_cascade(
                &rule.declarations,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border_width,
            );
            apply_border_style_cascade(
                &rule.declarations,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border_style,
            );
            apply_border_color_cascade(
                &rule.declarations,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border_color,
            );
            apply_logical_border_cascade(
                &rule.declarations.logical_border,
                rule.selector.specificity,
                rule.order,
                false,
                NativeLogicalBorderCandidateTargets {
                    border: &mut logical_border,
                    width: &mut logical_border_width,
                    style: &mut logical_border_style,
                    color: &mut logical_border_color,
                },
            );
            apply_border_radius_cascade(
                &rule.declarations,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border_radius,
            );
            apply_logical_border_radius_cascade(
                &rule.declarations.logical_border_radius,
                rule.selector.specificity,
                rule.order,
                false,
                &mut logical_border_radius,
            );
            apply_logical_box_model_cascade(
                &rule.declarations.logical_box_model,
                rule.selector.specificity,
                rule.order,
                false,
                &mut logical_padding,
                &mut logical_margin,
            );
            apply_local_important_cascade_edges(
                &rule.declarations.padding,
                &rule.declarations.box_model_importance.padding,
                &rule.declarations.padding_order,
                rule.selector.specificity,
                rule.order,
                false,
                &mut padding,
            );
            apply_local_important_cascade_edges(
                &rule.declarations.margin,
                &rule.declarations.box_model_importance.margin,
                &rule.declarations.margin_order,
                rule.selector.specificity,
                rule.order,
                false,
                &mut margin,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.box_sizing,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.box_model_importance.box_sizing,
                &mut box_sizing,
            );
            apply_paint_cascade_declaration(
                rule.declarations.color,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.color_important,
                &mut color,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.overflow_x,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.overflow_importance.x,
                &mut overflow_x,
            );
            apply_local_important_cascade_declaration(
                rule.declarations.overflow_y,
                rule.selector.specificity,
                rule.order,
                false,
                rule.declarations.overflow_importance.y,
                &mut overflow_y,
            );
        }

        if node.inline_style_allowed()
            && let Some(inline_style) = node.attribute("style")
        {
            let declarations = parse_declarations(inline_style);
            apply_local_important_cascade_declaration(
                declarations.display,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.display,
                &mut display,
            );
            apply_local_important_cascade_declaration(
                declarations.position,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.position,
                &mut position,
            );
            apply_local_important_cascade_declaration(
                declarations.z_index,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.z_index,
                &mut z_index,
            );
            apply_text_cascade_declaration(
                declarations.pointer_events,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.pointer_events,
                &mut pointer_events,
            );
            apply_local_important_cascade_declaration(
                declarations.top,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.top,
                &mut top,
            );
            apply_local_important_cascade_declaration(
                declarations.right,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.right,
                &mut right,
            );
            apply_local_important_cascade_declaration(
                declarations.bottom,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.bottom,
                &mut bottom,
            );
            apply_local_important_cascade_declaration(
                declarations.left,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.left,
                &mut left,
            );
            apply_local_important_cascade_declaration(
                declarations.grid_template_columns,
                u16::MAX,
                usize::MAX,
                true,
                declarations.grid_template_columns_important,
                &mut grid_template_columns,
            );
            apply_local_important_cascade_declaration(
                declarations.grid_template_rows,
                u16::MAX,
                usize::MAX,
                true,
                declarations.grid_template_rows_important,
                &mut grid_template_rows,
            );
            apply_local_important_cascade_declaration(
                declarations.visibility,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.visibility,
                &mut visibility,
            );
            apply_local_important_cascade_declaration(
                declarations.opacity,
                u16::MAX,
                usize::MAX,
                true,
                declarations.local_importance.opacity,
                &mut opacity,
            );
            apply_text_cascade_declaration(
                declarations.white_space,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.white_space,
                &mut white_space,
            );
            apply_text_cascade_declaration(
                declarations.text_align,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_align,
                &mut text_align,
            );
            apply_text_cascade_declaration(
                declarations.text_align_last,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_align_last,
                &mut text_align_last,
            );
            apply_text_cascade_declaration(
                declarations.text_justify,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_justify,
                &mut text_justify,
            );
            apply_text_cascade_declaration(
                declarations.direction,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.direction,
                &mut direction,
            );
            apply_text_cascade_declaration(
                declarations.text_decoration,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_decoration,
                &mut text_decoration,
            );
            apply_text_cascade_declaration(
                declarations.text_decoration_style,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_decoration_style,
                &mut text_decoration_style,
            );
            apply_text_cascade_declaration(
                declarations.text_decoration_skip_ink,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_decoration_skip_ink,
                &mut text_decoration_skip_ink,
            );
            apply_text_cascade_declaration(
                declarations.text_decoration_skip_spaces,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_decoration_skip_spaces,
                &mut text_decoration_skip_spaces,
            );
            apply_text_cascade_declaration(
                declarations.text_decoration_thickness,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_decoration_thickness,
                &mut text_decoration_thickness,
            );
            apply_text_cascade_declaration(
                declarations.text_underline_offset,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_underline_offset,
                &mut text_underline_offset,
            );
            apply_paint_cascade_declaration(
                declarations.text_decoration_color,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_decoration_color_important,
                &mut text_decoration_color,
            );
            apply_text_cascade_declaration(
                declarations.text_transform,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_transform,
                &mut text_transform,
            );
            apply_text_cascade_declaration(
                declarations.font_variant_ligatures,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variant_ligatures,
                &mut font_variant_ligatures,
            );
            apply_text_cascade_declaration(
                declarations.font_variant_caps,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variant_caps,
                &mut font_variant_caps,
            );
            apply_text_cascade_declaration(
                declarations.font_variant_position,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variant_position,
                &mut font_variant_position,
            );
            apply_text_cascade_declaration(
                declarations.font_variant_alternates,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variant_alternates,
                &mut font_variant_alternates,
            );
            apply_text_cascade_declaration(
                declarations.font_language_override,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_language_override,
                &mut font_language_override,
            );
            apply_text_cascade_declaration(
                declarations.font_variation_settings,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variation_settings,
                &mut font_variation_settings,
            );
            apply_text_cascade_declaration(
                declarations.font_variant_east_asian,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variant_east_asian,
                &mut font_variant_east_asian,
            );
            apply_text_cascade_declaration(
                declarations.font_variant_numeric,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_variant_numeric,
                &mut font_variant_numeric,
            );
            apply_text_cascade_declaration(
                declarations.font_feature_settings,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_feature_settings,
                &mut font_feature_settings,
            );
            apply_text_cascade_declaration(
                declarations.font_kerning,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_kerning,
                &mut font_kerning,
            );
            apply_text_cascade_declaration(
                declarations.font_optical_sizing,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_optical_sizing,
                &mut font_optical_sizing,
            );
            apply_text_cascade_declaration(
                declarations.font_palette,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_palette,
                &mut font_palette,
            );
            apply_text_cascade_declaration(
                declarations.font_weight,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_weight,
                &mut font_weight,
            );
            apply_text_cascade_declaration(
                declarations.font_style,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_style,
                &mut font_style,
            );
            apply_text_cascade_declaration(
                declarations.font_stretch,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_stretch,
                &mut font_stretch,
            );
            apply_text_cascade_declaration(
                declarations.font_family,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_family,
                &mut font_family,
            );
            apply_text_cascade_declaration(
                declarations.font_size,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.font_size,
                &mut font_size,
            );
            apply_text_cascade_declaration(
                declarations.word_break,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.word_break,
                &mut word_break,
            );
            apply_text_cascade_declaration(
                declarations.text_overflow,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_overflow,
                &mut text_overflow,
            );
            apply_text_cascade_declaration(
                declarations.vertical_align,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.vertical_align,
                &mut vertical_align,
            );
            apply_text_cascade_declaration(
                declarations.text_indent,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.text_indent,
                &mut text_indent,
            );
            apply_text_cascade_declaration(
                declarations.word_spacing,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.word_spacing,
                &mut word_spacing,
            );
            apply_text_cascade_declaration(
                declarations.letter_spacing,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.letter_spacing,
                &mut letter_spacing,
            );
            apply_gap_declarations(declarations, u16::MAX, usize::MAX, true, &mut gap);
            apply_flex_cascade_declaration(
                declarations.justify_content,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.justify_content,
                &mut justify_content,
            );
            apply_flex_cascade_declaration(
                declarations.order,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.order,
                &mut flex_item_order,
            );
            apply_flex_cascade_declaration(
                declarations.flex_grow,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.flex_grow,
                &mut flex_grow,
            );
            apply_flex_cascade_declaration(
                declarations.flex_shrink,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.flex_shrink,
                &mut flex_shrink,
            );
            apply_flex_cascade_declaration(
                declarations.flex_basis,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.flex_basis,
                &mut flex_basis,
            );
            apply_flex_cascade_declaration(
                declarations.align_items,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.align_items,
                &mut align_items,
            );
            apply_flex_cascade_declaration(
                declarations.align_self,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.align_self,
                &mut align_self,
            );
            apply_flex_cascade_declaration(
                declarations.align_content,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.align_content,
                &mut align_content,
            );
            apply_flex_cascade_declaration(
                declarations.flex_direction,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.flex_direction,
                &mut flex_direction,
            );
            apply_flex_cascade_declaration(
                declarations.flex_wrap,
                u16::MAX,
                usize::MAX,
                true,
                declarations.flex_importance.flex_wrap,
                &mut flex_wrap,
            );
            apply_local_important_cascade_declaration(
                declarations.width,
                u16::MAX,
                usize::MAX,
                true,
                declarations.dimension_importance.width,
                &mut width,
            );
            apply_local_important_cascade_declaration(
                declarations.height,
                u16::MAX,
                usize::MAX,
                true,
                declarations.dimension_importance.height,
                &mut height,
            );
            apply_local_important_cascade_declaration(
                declarations.min_width,
                u16::MAX,
                usize::MAX,
                true,
                declarations.dimension_importance.min_width,
                &mut min_width,
            );
            apply_local_important_cascade_declaration(
                declarations.max_width,
                u16::MAX,
                usize::MAX,
                true,
                declarations.dimension_importance.max_width,
                &mut max_width,
            );
            apply_local_important_cascade_declaration(
                declarations.min_height,
                u16::MAX,
                usize::MAX,
                true,
                declarations.dimension_importance.min_height,
                &mut min_height,
            );
            apply_local_important_cascade_declaration(
                declarations.max_height,
                u16::MAX,
                usize::MAX,
                true,
                declarations.dimension_importance.max_height,
                &mut max_height,
            );
            apply_text_cascade_declaration(
                declarations.line_height,
                u16::MAX,
                usize::MAX,
                true,
                declarations.text_importance.line_height,
                &mut line_height,
            );
            apply_paint_cascade_declaration(
                declarations.background_color,
                u16::MAX,
                usize::MAX,
                true,
                declarations.background_color_important,
                &mut background_color,
            );
            apply_paint_cascade_declaration(
                declarations.background_image,
                u16::MAX,
                usize::MAX,
                true,
                declarations.background_image_important,
                &mut background_image,
            );
            apply_paint_cascade_declaration(
                declarations.background_repeat,
                u16::MAX,
                usize::MAX,
                true,
                declarations.background_repeat_important,
                &mut background_repeat,
            );
            apply_paint_cascade_declaration(
                declarations.background_position,
                u16::MAX,
                usize::MAX,
                true,
                declarations.background_position_important,
                &mut background_position,
            );
            apply_paint_cascade_declaration(
                declarations.background_size,
                u16::MAX,
                usize::MAX,
                true,
                declarations.background_size_important,
                &mut background_size,
            );
            apply_local_cascade_edges(
                &declarations.border,
                u16::MAX,
                usize::MAX,
                true,
                &mut border,
            );
            apply_border_width_cascade(
                &declarations,
                u16::MAX,
                usize::MAX,
                true,
                &mut border_width,
            );
            apply_border_style_cascade(
                &declarations,
                u16::MAX,
                usize::MAX,
                true,
                &mut border_style,
            );
            apply_border_color_cascade(
                &declarations,
                u16::MAX,
                usize::MAX,
                true,
                &mut border_color,
            );
            apply_logical_border_cascade(
                &declarations.logical_border,
                u16::MAX,
                usize::MAX,
                true,
                NativeLogicalBorderCandidateTargets {
                    border: &mut logical_border,
                    width: &mut logical_border_width,
                    style: &mut logical_border_style,
                    color: &mut logical_border_color,
                },
            );
            apply_border_radius_cascade(
                &declarations,
                u16::MAX,
                usize::MAX,
                true,
                &mut border_radius,
            );
            apply_logical_border_radius_cascade(
                &declarations.logical_border_radius,
                u16::MAX,
                usize::MAX,
                true,
                &mut logical_border_radius,
            );
            apply_logical_box_model_cascade(
                &declarations.logical_box_model,
                u16::MAX,
                usize::MAX,
                true,
                &mut logical_padding,
                &mut logical_margin,
            );
            apply_local_important_cascade_edges(
                &declarations.padding,
                &declarations.box_model_importance.padding,
                &declarations.padding_order,
                u16::MAX,
                usize::MAX,
                true,
                &mut padding,
            );
            apply_local_important_cascade_edges(
                &declarations.margin,
                &declarations.box_model_importance.margin,
                &declarations.margin_order,
                u16::MAX,
                usize::MAX,
                true,
                &mut margin,
            );
            apply_local_important_cascade_declaration(
                declarations.box_sizing,
                u16::MAX,
                usize::MAX,
                true,
                declarations.box_model_importance.box_sizing,
                &mut box_sizing,
            );
            apply_paint_cascade_declaration(
                declarations.color,
                u16::MAX,
                usize::MAX,
                true,
                declarations.color_important,
                &mut color,
            );
            apply_local_important_cascade_declaration(
                declarations.overflow_x,
                u16::MAX,
                usize::MAX,
                true,
                declarations.overflow_importance.x,
                &mut overflow_x,
            );
            apply_local_important_cascade_declaration(
                declarations.overflow_y,
                u16::MAX,
                usize::MAX,
                true,
                declarations.overflow_importance.y,
                &mut overflow_y,
            );
        }

        let resolved_direction =
            resolve_direction(*direction, inherited.direction, custom_properties);
        project_logical_border_candidates(
            NativeLogicalBorderCandidateSources {
                border: &logical_border,
                width: &logical_border_width,
                style: &logical_border_style,
                color: &logical_border_color,
            },
            resolved_direction,
            NativePhysicalBorderCandidateTargets {
                border: &mut border,
                width: &mut border_width,
                style: &mut border_style,
                color: &mut border_color,
            },
        );
        project_logical_border_radius_candidates(
            &logical_border_radius,
            resolved_direction,
            &mut border_radius,
        );
        project_logical_padding_candidates(
            &logical_padding,
            resolved_direction,
            inherited.direction,
            inherited.padding,
            padding,
        );
        project_logical_margin_candidates(
            &logical_margin,
            resolved_direction,
            inherited.direction,
            inherited.margin,
            margin,
        );

        let resolved_padding = std::array::from_fn(|index| {
            resolve_padding_candidates(padding[index], inherited.padding[index], custom_properties)
        });
        let resolved_margin = std::array::from_fn(|index| {
            resolve_margin_candidates(margin[index], inherited.margin[index], custom_properties)
        });
        let resolved_border = (*border)
            .map(|candidates| resolve_local_border_declaration(candidates, custom_properties));
        let resolved_border_width: [Option<u32>; 4] = std::array::from_fn(|index| {
            resolve_local_border_width_declaration(
                border_width[index],
                inherited.border_width[index],
                custom_properties,
            )
        });
        let border_widths = std::array::from_fn(|index| resolved_border_width[index].unwrap_or(0));
        let resolved_border_style: [Option<NativeBorderStyleValue>; 4] =
            std::array::from_fn(|index| {
                resolve_local_border_style_declaration(
                    border_style[index],
                    inherited.border_style[index],
                    custom_properties,
                )
            });
        let border_styles = std::array::from_fn(|index| {
            resolved_border_style[index].unwrap_or(NativeBorderStyleValue::None)
        });
        let resolved_color =
            resolve_local_color_declaration(*color, inherited.color, custom_properties);
        let current_color = resolved_color.unwrap_or(NativeColor::BLACK);
        let resolved_background_color = resolve_local_background_color_declaration(
            *background_color,
            inherited.background_color,
            current_color,
            custom_properties,
        );
        let resolved_background_image =
            resolve_local_background_image_declaration(*background_image, custom_properties);
        let resolved_background_repeat =
            resolve_local_background_repeat_declaration(*background_repeat, custom_properties);
        let resolved_background_position =
            resolve_local_background_position_declaration(*background_position, custom_properties);
        let resolved_background_size =
            resolve_local_background_size_declaration(*background_size, custom_properties);
        let resolved_border_color: [Option<NativeColor>; 4] = std::array::from_fn(|index| {
            resolve_local_border_color_declaration(
                border_color[index],
                current_color,
                inherited.border_color[index],
                custom_properties,
            )
        });
        let border_colors =
            std::array::from_fn(|index| resolved_border_color[index].unwrap_or(NativeColor::BLACK));
        let resolved_border = std::array::from_fn(|index| {
            match (
                resolved_border[index],
                resolved_border_width[index],
                resolved_border_style[index],
            ) {
                (Some(_), width, Some(NativeBorderStyleValue::Paint(style))) => {
                    Some(NativeBorderSide {
                        width: width.unwrap_or(0),
                        style,
                        color: resolved_border_color[index].unwrap_or(NativeColor::BLACK),
                    })
                }
                (None, Some(width), Some(NativeBorderStyleValue::Paint(style))) => {
                    Some(NativeBorderSide {
                        width,
                        style,
                        color: resolved_border_color[index].unwrap_or(NativeColor::BLACK),
                    })
                }
                _ => None,
            }
        });

        let resolved_box_sizing =
            resolve_box_sizing(*box_sizing, inherited.box_sizing, custom_properties);
        let resolved_overflow_x =
            resolve_overflow_axis(*overflow_x, inherited.overflow_x, custom_properties);
        let resolved_overflow_y =
            resolve_overflow_axis(*overflow_y, inherited.overflow_y, custom_properties);
        NativeComputedStyle {
            display: resolve_display(*display, custom_properties),
            position: resolve_position(*position, custom_properties),
            z_index: resolve_z_index(*z_index, custom_properties),
            pointer_events: resolve_pointer_events(
                *pointer_events,
                inherited.pointer_events,
                custom_properties,
            ),
            top: resolve_position_offset(*top, custom_properties),
            right: resolve_position_offset(*right, custom_properties),
            bottom: resolve_position_offset(*bottom, custom_properties),
            left: resolve_position_offset(*left, custom_properties),
            grid_template_columns: resolve_grid_track_list(
                *grid_template_columns,
                custom_properties,
            ),
            grid_template_rows: resolve_grid_track_list(*grid_template_rows, custom_properties),
            visibility_hidden: resolve_visibility(*visibility, custom_properties)
                == VisibilityValue::Hidden,
            opacity: resolve_opacity(*opacity, custom_properties),
            white_space: resolve_white_space(
                *white_space,
                inherited.white_space,
                custom_properties,
            ),
            text_align: resolve_text_align(*text_align, inherited.text_align, custom_properties),
            text_align_last: resolve_text_align_last(
                *text_align_last,
                inherited.text_align_last,
                custom_properties,
            ),
            text_justify: resolve_text_justify(
                *text_justify,
                inherited.text_justify,
                custom_properties,
            ),
            justify_content: resolve_justify_content(
                *justify_content,
                inherited.justify_content,
                custom_properties,
            ),
            align_items: resolve_align_items(
                *align_items,
                inherited.align_items,
                custom_properties,
            ),
            align_self: resolve_align_self(*align_self, inherited.align_self, custom_properties),
            align_content: resolve_align_content(
                *align_content,
                inherited.align_content,
                custom_properties,
            ),
            flex_direction: resolve_flex_direction(
                *flex_direction,
                inherited.flex_direction,
                custom_properties,
            ),
            direction: resolved_direction,
            flex_wrap: resolve_flex_wrap(*flex_wrap, inherited.flex_wrap, custom_properties),
            flex_item_order: resolve_flex_item_order(
                *flex_item_order,
                inherited.flex_item_order,
                custom_properties,
            ),
            flex_grow: resolve_flex_grow(*flex_grow, inherited.flex_grow, custom_properties),
            flex_shrink: resolve_flex_shrink(
                *flex_shrink,
                inherited.flex_shrink,
                custom_properties,
            ),
            flex_basis: resolve_flex_basis(*flex_basis, inherited.flex_basis, custom_properties),
            text_decoration: resolve_text_decoration(
                *text_decoration,
                inherited.text_decoration,
                custom_properties,
            ),
            text_decoration_style: resolve_text_decoration_style(
                *text_decoration_style,
                inherited.text_decoration_style,
                custom_properties,
            ),
            text_decoration_skip_ink: resolve_text_decoration_skip_ink(
                *text_decoration_skip_ink,
                inherited.text_decoration_skip_ink,
                custom_properties,
            ),
            text_decoration_skip_spaces: resolve_text_decoration_skip_spaces(
                *text_decoration_skip_spaces,
                inherited.text_decoration_skip_spaces,
                custom_properties,
            ),
            text_decoration_thickness: resolve_text_decoration_thickness(
                *text_decoration_thickness,
                inherited.text_decoration_thickness,
                custom_properties,
            ),
            text_underline_offset: resolve_text_underline_offset(
                *text_underline_offset,
                inherited.text_underline_offset,
                custom_properties,
            ),
            text_decoration_color: resolve_text_decoration_color(
                *text_decoration_color,
                current_color,
                inherited.text_decoration_color,
                custom_properties,
            ),
            text_transform: resolve_text_transform(
                *text_transform,
                inherited.text_transform,
                custom_properties,
            ),
            font_variant_ligatures: resolve_font_variant_ligatures(
                *font_variant_ligatures,
                inherited.font_variant_ligatures,
                custom_properties,
            ),
            font_variant_caps: resolve_font_variant_caps(
                *font_variant_caps,
                inherited.font_variant_caps,
                custom_properties,
            ),
            font_variant_position: resolve_font_variant_position(
                *font_variant_position,
                inherited.font_variant_position,
                custom_properties,
            ),
            font_variant_alternates: resolve_font_variant_alternates(
                *font_variant_alternates,
                inherited.font_variant_alternates,
                custom_properties,
            ),
            font_language_override: resolve_font_language_override(
                *font_language_override,
                inherited.font_language_override,
                custom_properties,
            ),
            font_variation_settings: resolve_font_variation_settings(
                *font_variation_settings,
                inherited.font_variation_settings,
                custom_properties,
            ),
            font_variant_east_asian: resolve_font_variant_east_asian(
                *font_variant_east_asian,
                inherited.font_variant_east_asian,
                custom_properties,
            ),
            font_variant_numeric: resolve_font_variant_numeric(
                *font_variant_numeric,
                inherited.font_variant_numeric,
                custom_properties,
            ),
            font_feature_settings: resolve_font_feature_settings(
                *font_feature_settings,
                inherited.font_feature_settings,
                custom_properties,
            ),
            font_kerning: resolve_font_kerning(
                *font_kerning,
                inherited.font_kerning,
                custom_properties,
            ),
            font_optical_sizing: resolve_font_optical_sizing(
                *font_optical_sizing,
                inherited.font_optical_sizing,
                custom_properties,
            ),
            font_palette: resolve_font_palette(
                *font_palette,
                inherited.font_palette,
                custom_properties,
            ),
            font_weight: resolve_font_weight(
                *font_weight,
                inherited.font_weight,
                custom_properties,
            ),
            font_style: resolve_font_style(*font_style, inherited.font_style, custom_properties),
            font_stretch: resolve_font_stretch(
                *font_stretch,
                inherited.font_stretch,
                custom_properties,
            ),
            font_family: resolve_font_family(
                *font_family,
                inherited.font_family,
                custom_properties,
            ),
            font_size: resolve_font_size(
                *font_size,
                inherited.font_size,
                inherited.root_font_size,
                inherited.viewport,
                custom_properties,
            ),
            word_break: resolve_word_break(*word_break, inherited.word_break, custom_properties),
            text_overflow: resolve_text_overflow(
                *text_overflow,
                inherited.text_overflow,
                custom_properties,
            ),
            vertical_align: resolve_vertical_align(
                *vertical_align,
                inherited.vertical_align,
                custom_properties,
            ),
            text_indent: resolve_text_indent(
                *text_indent,
                inherited.text_indent,
                custom_properties,
            ),
            word_spacing: resolve_word_spacing(
                *word_spacing,
                inherited.word_spacing,
                custom_properties,
            ),
            letter_spacing: resolve_letter_spacing(
                *letter_spacing,
                inherited.letter_spacing,
                custom_properties,
            ),
            gap: resolve_gap_axis(
                (*gap).shorthand_column,
                (*gap).column_gap,
                inherited.column_gap,
                custom_properties,
            ),
            row_gap: resolve_gap_axis(
                (*gap).shorthand_row,
                (*gap).row_gap,
                inherited.row_gap,
                custom_properties,
            ),
            width: resolve_dimension(*width, inherited.width, custom_properties),
            height: resolve_dimension(*height, inherited.height, custom_properties),
            min_width: resolve_dimension(*min_width, inherited.min_width, custom_properties),
            max_width: resolve_dimension(*max_width, inherited.max_width, custom_properties),
            min_height: resolve_dimension(*min_height, inherited.min_height, custom_properties),
            max_height: resolve_dimension(*max_height, inherited.max_height, custom_properties),
            line_height: resolve_line_height(
                *line_height,
                inherited.line_height,
                custom_properties,
            ),
            background_color: resolved_background_color,
            background_image: resolved_background_image,
            background_repeat: resolved_background_repeat,
            background_position: resolved_background_position,
            background_size: resolved_background_size,
            border: NativeBorder::from_sides(resolved_border),
            border_colors: Some(border_colors),
            border_widths,
            border_styles,
            border_radius: resolve_local_border_radius_declaration(
                *border_radius,
                inherited.border_radius,
                custom_properties,
            ),
            padding: NativeBoxEdges::from_values(resolved_padding),
            margin: NativeBoxEdges::from_margin_values(resolved_margin),
            margin_auto: NativeAutoEdges::from_values(resolved_margin),
            box_sizing: resolved_box_sizing,
            color: resolved_color,
            overflow_x: resolved_overflow_x,
            overflow_y: resolved_overflow_y,
            overflow_clip_x: !matches!(resolved_overflow_x, OverflowValue::Other),
            overflow_clip_y: !matches!(resolved_overflow_y, OverflowValue::Other),
        }
    }
}
