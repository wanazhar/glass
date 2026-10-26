use super::*;

thread_local! {
    static LAYOUT_TEXT_METRICS_DIAGNOSTIC: std::cell::Cell<(usize, u128)> = const {
        std::cell::Cell::new((0, 0))
    };
}

pub(crate) fn reset_layout_text_metrics_diagnostic() {
    LAYOUT_TEXT_METRICS_DIAGNOSTIC.with(|diagnostic| diagnostic.set((0, 0)));
}

pub(crate) fn take_layout_text_metrics_diagnostic() -> (usize, u128) {
    LAYOUT_TEXT_METRICS_DIAGNOSTIC.with(|diagnostic| diagnostic.replace((0, 0)))
}

impl NativeDocument {
    #[track_caller]
    pub(super) fn invalidate_layout_styles(&mut self) {
        if crate::native_engine::content_process::runtime_diagnostic_trace_enabled() {
            let caller = std::panic::Location::caller();
            crate::native_engine::content_process::ContentLoadStage::begin_with_resource(
                0,
                "layout_styles_invalidated",
                Some(caller.line() as usize),
                Some(caller.file()),
            )
            .finish(None);
        }
        self.computed_styles = None;
        self.layout_style_cache.clear();
    }

    pub(crate) fn computed_style_for_layout(&self, id: NativeNodeId) -> NativeComputedStyle {
        self.computed_style_for_layout_internal(id, true)
    }

    #[cfg(test)]
    pub(crate) fn computed_style_for_layout_without_parent_cache(
        &self,
        id: NativeNodeId,
    ) -> NativeComputedStyle {
        self.computed_style_for_layout_internal(id, false)
    }

    fn computed_style_for_layout_internal(
        &self,
        id: NativeNodeId,
        use_cached_parent: bool,
    ) -> NativeComputedStyle {
        if self.node(id).is_none() {
            return NativeComputedStyle::default();
        }
        if let Some(computed_styles) = &self.computed_styles {
            return computed_styles
                .get(id.index as usize)
                .copied()
                .unwrap_or_default();
        }
        // The per-node cache is invalidated with every style-affecting
        // mutation. Return a valid hit before walking ancestors: host snapshot
        // export and repeated layout passes revisit each node after its style
        // has already been computed, and rebuilding inherited state there is
        // redundant.
        if let Some(style) = self
            .layout_style_cache
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&id)
            .copied()
        {
            return style;
        }
        let mut chain = Vec::new();
        let mut current = Some(id);
        for _ in 0..=MAX_NATIVE_DOM_DEPTH {
            let Some(current_id) = current else {
                break;
            };
            chain.push(current_id);
            current = self.node(current_id).and_then(NativeNode::parent);
        }

        let mut inherited_color = Some(NativeColor::BLACK);
        let mut inherited_background_color = None;
        let mut inherited_text_decoration_color = NativeColor::BLACK;
        let mut inherited_border_color = [NativeColor::BLACK; 4];
        let mut inherited_border_width = [0; 4];
        let mut inherited_border_style = [NativeBorderStyleValue::None; 4];
        let mut inherited_border_radius = NativeBorderRadius::default();
        let mut inherited_padding = [0; 4];
        let mut inherited_margin = [NativeMarginValue::Length(0); 4];
        let mut inherited_box_sizing = NativeBoxSizing::ContentBox;
        let mut inherited_width = None;
        let mut inherited_height = None;
        let mut inherited_min_width = None;
        let mut inherited_max_width = None;
        let mut inherited_min_height = None;
        let mut inherited_max_height = None;
        let mut inherited_justify_content = JustifyContentValue::FlexStart;
        let mut inherited_align_items = AlignItemsValue::FlexStart;
        let mut inherited_align_self = AlignSelfValue::Auto;
        let mut inherited_align_content = AlignContentValue::FlexStart;
        let mut inherited_flex_direction = FlexDirectionValue::Row;
        let mut inherited_flex_wrap = FlexWrapValue::NoWrap;
        let mut inherited_flex_item_order = NativeOrderValue::default();
        let mut inherited_row_gap = 0;
        let mut inherited_column_gap = 0;
        let mut inherited_direction = DirectionValue::Ltr;
        let mut inherited_flex_grow = 0;
        let mut inherited_flex_shrink = 1;
        let mut inherited_flex_basis = FlexBasisValue::Auto;
        let mut inherited_white_space = WhiteSpaceValue::Normal;
        let mut inherited_line_height = None;
        let mut inherited_text_align = TextAlignValue::Left;
        let mut inherited_text_align_last = TextAlignLastValue::Auto;
        let mut inherited_text_justify = TextJustifyValue::Auto;
        let mut inherited_text_decoration = TextDecorationValue::none();
        let mut inherited_text_decoration_style = NativeTextDecorationStyle::Solid;
        let mut inherited_text_decoration_skip_ink = NativeTextDecorationSkipInk::Auto;
        let mut inherited_text_decoration_skip_spaces = NativeTextDecorationSkipSpaces::None;
        let mut inherited_text_decoration_thickness = 1;
        let mut inherited_text_underline_offset = 0;
        let mut inherited_text_transform = TextTransformValue::None;
        let mut inherited_font_variant_ligatures = NativeFontVariantLigatures::default();
        let mut inherited_font_variant_caps = NativeFontVariantCaps::Normal;
        let mut inherited_font_variant_position = NativeFontVariantPosition::Normal;
        let mut inherited_font_variant_alternates = NativeFontVariantAlternates::Normal;
        let mut inherited_font_language_override = NativeFontLanguageOverride::default();
        let mut inherited_font_variation_settings = NativeFontVariationSettings::default();
        let mut inherited_font_variant_east_asian = NativeFontVariantEastAsian::default();
        let mut inherited_font_variant_numeric = NativeFontVariantNumeric::default();
        let mut inherited_font_feature_settings = NativeFontFeatureSettings::default();
        let mut inherited_font_kerning = NativeFontKerning::Auto;
        let mut inherited_font_optical_sizing = NativeFontOpticalSizing::Auto;
        let mut inherited_font_palette = NativeFontPalette::Normal;
        let mut inherited_font_weight = FontWeightValue::Normal;
        let mut inherited_font_style = FontStyleValue::Normal;
        let mut inherited_font_stretch = super::super::css::NativeFontStretchRange::default();
        let mut inherited_font_family = NativeFontFamilyList::default();
        let mut inherited_font_size = super::super::font::DEFAULT_NATIVE_FONT_SIZE;
        let mut inherited_root_font_size = super::super::font::DEFAULT_NATIVE_FONT_SIZE;
        let mut root_element_seen = false;
        let mut inherited_word_break = WordBreakValue::Normal;
        let mut inherited_text_overflow = TextOverflowValue::Clip;
        let mut inherited_overflow_x = OverflowValue::Other;
        let mut inherited_overflow_y = OverflowValue::Other;
        let mut inherited_vertical_align = VerticalAlignValue::Baseline;
        let mut inherited_text_indent = 0;
        let mut inherited_word_spacing = 0;
        let mut inherited_letter_spacing = 0;
        let mut inherited_pointer_events = NativePointerEventsValue::Auto;
        let mut inherited_custom_properties = Arc::new(BTreeMap::new());
        macro_rules! inherit_from_style {
            ($style:expr) => {{
                let style = $style;
                inherited_color = style.color().or(inherited_color);
                inherited_background_color = style.background_color();
                inherited_text_decoration_color = style
                    .text_decoration_color()
                    .unwrap_or(style.color().unwrap_or(NativeColor::BLACK));
                inherited_border_color = style.border_colors();
                inherited_border_width = style.border_widths();
                inherited_border_style = style.border_styles();
                inherited_border_radius = style.border_radius();
                inherited_padding = [
                    style.padding().top(),
                    style.padding().right(),
                    style.padding().bottom(),
                    style.padding().left(),
                ];
                inherited_margin = style.margin_values();
                inherited_box_sizing = style.box_sizing();
                inherited_width = style.width();
                inherited_height = style.height();
                inherited_min_width = style.min_width();
                inherited_max_width = style.max_width();
                inherited_min_height = style.min_height();
                inherited_max_height = style.max_height();
                inherited_justify_content = style.justify_content();
                inherited_align_items = style.align_items();
                inherited_align_self = style.align_self();
                inherited_align_content = style.align_content();
                inherited_flex_direction = style.flex_direction();
                inherited_flex_wrap = style.flex_wrap();
                inherited_flex_item_order = style.flex_item_order();
                inherited_row_gap = style.row_gap();
                inherited_column_gap = style.column_gap();
                inherited_direction = style.direction();
                inherited_flex_grow = style.flex_grow();
                inherited_flex_shrink = style.flex_shrink();
                inherited_flex_basis = style.flex_basis();
                inherited_white_space = style.white_space();
                inherited_line_height = style.line_height().or(inherited_line_height);
                inherited_text_align = style.text_align();
                inherited_text_align_last = style.text_align_last();
                inherited_text_justify = style.text_justify();
                inherited_text_decoration = style.text_decoration();
                inherited_text_decoration_style = style.text_decoration_style();
                inherited_text_decoration_skip_ink = style.text_decoration_skip_ink();
                inherited_text_decoration_skip_spaces = style.text_decoration_skip_spaces();
                inherited_text_decoration_thickness = style.text_decoration_thickness();
                inherited_text_underline_offset = style.text_underline_offset();
                inherited_text_transform = style.text_transform();
                inherited_font_variant_ligatures = style.font_variant_ligatures();
                inherited_font_variant_caps = style.font_variant_caps();
                inherited_font_variant_position = style.font_variant_position();
                inherited_font_variant_alternates = style.font_variant_alternates();
                inherited_font_language_override = style.font_language_override();
                inherited_font_variation_settings = style.font_variation_settings();
                inherited_font_variant_east_asian = style.font_variant_east_asian();
                inherited_font_variant_numeric = style.font_variant_numeric();
                inherited_font_feature_settings = style.font_feature_settings();
                inherited_font_kerning = style.font_kerning();
                inherited_font_optical_sizing = style.font_optical_sizing();
                inherited_font_palette = style.font_palette();
                inherited_font_weight = style.font_weight();
                inherited_font_style = style.font_style();
                inherited_font_stretch = style.font_stretch();
                inherited_font_family = style.font_family();
                inherited_font_size = style.font_size();
                inherited_word_break = style.word_break();
                inherited_text_overflow = style.text_overflow();
                inherited_overflow_x = style.overflow_x();
                inherited_overflow_y = style.overflow_y();
                inherited_vertical_align = style.vertical_align();
                inherited_text_indent = style.text_indent();
                inherited_word_spacing = style.word_spacing();
                inherited_letter_spacing = style.letter_spacing();
                inherited_pointer_events = style.pointer_events();
            }};
        }
        // Reuse the rule-index scratch across the ancestor chain. Each node's
        // style and custom-property cascade share the same matching candidates.
        let mut rule_candidates = Vec::new();
        // Snapshot transfer walks the arena in preorder. When the target's
        // parent already has both cached style and inherited variables, start
        // at the target directly instead of rebuilding the entire ancestor
        // chain for every subsequent node. Arbitrary query orders still use
        // the complete fallback chain below.
        let cached_parent = use_cached_parent
            .then(|| {
                self.node(id)
                    .and_then(NativeNode::parent)
                    .and_then(|parent_id| {
                        let style = self
                            .layout_style_cache
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .get(&parent_id)
                            .copied()?;
                        let custom_properties = self
                            .layout_style_cache
                            .1
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .get(&parent_id)
                            .cloned()?;
                        Some((parent_id, style, custom_properties))
                    })
            })
            .flatten();
        if let Some((parent_id, parent_style, custom_properties)) = cached_parent {
            let root_element = self.node(self.root).and_then(|root| {
                root.children().iter().find_map(|child| {
                    self.node(*child)
                        .filter(|node| node.element_name().is_some())
                        .map(NativeNode::id)
                })
            });
            if root_element == Some(parent_id) {
                inherited_root_font_size = parent_style.font_size();
                root_element_seen = true;
            } else if let Some(root_element) = root_element
                && let Some(root_style) = self
                    .layout_style_cache
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(&root_element)
                    .copied()
            {
                inherited_root_font_size = root_style.font_size();
                root_element_seen = true;
            }
            inherit_from_style!(parent_style);
            inherited_custom_properties = custom_properties;
            chain.clear();
            chain.push(id);
        }
        for current_id in chain.into_iter().rev() {
            let Some(_) = self.node(current_id) else {
                continue;
            };
            let is_root_element = self
                .node(current_id)
                .is_some_and(|node| node.element_name().is_some());
            let cached_custom_properties = {
                self.layout_style_cache
                    .1
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(&current_id)
                    .cloned()
            };
            let cached_style = self
                .layout_style_cache
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&current_id)
                .copied();
            if cached_custom_properties.is_none() || cached_style.is_none() {
                self.stylesheet.rule_candidates_for_node_in_document(
                    self,
                    current_id,
                    &mut rule_candidates,
                );
            }
            let custom_properties = cached_custom_properties.unwrap_or_else(|| {
                let local = self
                    .stylesheet
                    .custom_properties_for_node_in_document_with_candidates(
                        self,
                        current_id,
                        self.viewport,
                        &rule_candidates,
                    );
                let values = if local.is_empty() {
                    Arc::clone(&inherited_custom_properties)
                } else {
                    let mut values = (*inherited_custom_properties).clone();
                    values.extend(local);
                    Arc::new(values)
                };
                self.layout_style_cache
                    .1
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(current_id, Arc::clone(&values));
                values
            });
            inherited_custom_properties = Arc::clone(&custom_properties);
            let style = if let Some(style) = cached_style {
                style
            } else {
                let style_stage = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE").map(|_| {
                    crate::native_engine::content_process::ContentLoadStage::begin(
                        0,
                        "native_css_style_compute",
                    )
                });
                let style = self
                    .stylesheet
                    .computed_for_in_document_with_inheritance_and_custom_properties(
                        self,
                        current_id,
                        NativeInheritedStyle {
                            color: inherited_color,
                            background_color: inherited_background_color,
                            text_decoration_color: inherited_text_decoration_color,
                            border_color: inherited_border_color,
                            border_width: inherited_border_width,
                            border_style: inherited_border_style,
                            border_radius: inherited_border_radius,
                            padding: inherited_padding,
                            margin: inherited_margin,
                            box_sizing: inherited_box_sizing,
                            width: inherited_width,
                            height: inherited_height,
                            min_width: inherited_min_width,
                            max_width: inherited_max_width,
                            min_height: inherited_min_height,
                            max_height: inherited_max_height,
                            justify_content: inherited_justify_content,
                            align_items: inherited_align_items,
                            align_self: inherited_align_self,
                            align_content: inherited_align_content,
                            flex_direction: inherited_flex_direction,
                            flex_wrap: inherited_flex_wrap,
                            flex_item_order: inherited_flex_item_order,
                            row_gap: inherited_row_gap,
                            column_gap: inherited_column_gap,
                            direction: inherited_direction,
                            flex_grow: inherited_flex_grow,
                            flex_shrink: inherited_flex_shrink,
                            flex_basis: inherited_flex_basis,
                            white_space: inherited_white_space,
                            line_height: inherited_line_height,
                            text_align: inherited_text_align,
                            text_align_last: inherited_text_align_last,
                            text_justify: inherited_text_justify,
                            text_decoration: inherited_text_decoration,
                            text_decoration_style: inherited_text_decoration_style,
                            text_decoration_skip_ink: inherited_text_decoration_skip_ink,
                            text_decoration_skip_spaces: inherited_text_decoration_skip_spaces,
                            text_decoration_thickness: inherited_text_decoration_thickness,
                            text_underline_offset: inherited_text_underline_offset,
                            text_transform: inherited_text_transform,
                            font_variant_ligatures: inherited_font_variant_ligatures,
                            font_variant_caps: inherited_font_variant_caps,
                            font_variant_position: inherited_font_variant_position,
                            font_variant_alternates: inherited_font_variant_alternates,
                            font_language_override: inherited_font_language_override,
                            font_variation_settings: inherited_font_variation_settings,
                            font_variant_east_asian: inherited_font_variant_east_asian,
                            font_variant_numeric: inherited_font_variant_numeric,
                            font_feature_settings: inherited_font_feature_settings,
                            font_kerning: inherited_font_kerning,
                            font_optical_sizing: inherited_font_optical_sizing,
                            font_palette: inherited_font_palette,
                            font_weight: inherited_font_weight,
                            font_style: inherited_font_style,
                            font_stretch: inherited_font_stretch,
                            font_family: inherited_font_family,
                            font_size: inherited_font_size,
                            root_font_size: inherited_root_font_size,
                            viewport: self.viewport,
                            word_break: inherited_word_break,
                            text_overflow: inherited_text_overflow,
                            overflow_x: inherited_overflow_x,
                            overflow_y: inherited_overflow_y,
                            vertical_align: inherited_vertical_align,
                            text_indent: inherited_text_indent,
                            word_spacing: inherited_word_spacing,
                            letter_spacing: inherited_letter_spacing,
                            pointer_events: inherited_pointer_events,
                        },
                        &custom_properties,
                        self.viewport,
                        &rule_candidates,
                    );
                if let Some(stage) = style_stage {
                    stage.finish(None);
                }
                self.layout_style_cache
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(current_id, style);
                style
            };
            if !root_element_seen && is_root_element {
                inherited_root_font_size = style.font_size();
                root_element_seen = true;
            }
            inherit_from_style!(style);
            if current_id == id {
                return style;
            }
        }
        NativeComputedStyle::default()
    }

    pub(crate) fn is_hidden_for_layout(&self, id: NativeNodeId) -> bool {
        self.is_hidden(id)
    }

    pub(crate) fn text_metrics_for_layout(&self, id: NativeNodeId) -> NativeTextMetrics {
        let diagnostic_started =
            crate::native_engine::content_process::runtime_diagnostic_trace_enabled()
                .then(std::time::Instant::now);
        let style = self.computed_style_for_layout(id);
        let metrics = if style
            .font_family()
            .iter()
            .any(|family| !matches!(family, super::super::css::NativeFontFamilyValue::Fallback))
        {
            NativeTextMetrics::for_style_with_book_and_options(
                style.font_family(),
                style.font_size(),
                style.font_weight(),
                style.font_style(),
                NativeTextStyleOptions {
                    stretch: style.font_stretch().min,
                    ligatures: style.font_variant_ligatures(),
                    feature_settings: style.font_feature_settings(),
                    kerning: style.font_kerning(),
                    variant_caps: style.font_variant_caps(),
                    variant_position: style.font_variant_position(),
                    variant_numeric: style.font_variant_numeric(),
                    variant_alternates: style.font_variant_alternates(),
                    variant_east_asian: style.font_variant_east_asian(),
                    language_override: style.font_language_override(),
                    variation_settings: style.font_variation_settings(),
                    direction: style.direction(),
                },
                &self.font_book,
            )
        } else {
            NativeTextMetrics::fallback_with_style_options(
                style.font_size(),
                NativeTextStyleOptions {
                    stretch: style.font_stretch().min,
                    ligatures: style.font_variant_ligatures(),
                    feature_settings: style.font_feature_settings(),
                    kerning: style.font_kerning(),
                    variant_caps: style.font_variant_caps(),
                    variant_position: style.font_variant_position(),
                    variant_numeric: style.font_variant_numeric(),
                    variant_alternates: style.font_variant_alternates(),
                    variant_east_asian: style.font_variant_east_asian(),
                    language_override: style.font_language_override(),
                    variation_settings: style.font_variation_settings(),
                    direction: style.direction(),
                },
            )
        };
        let palette = style.font_palette();
        let metrics = metrics
            .with_optical_sizing(style.font_optical_sizing())
            .with_palette(self.stylesheet.resolve_font_palette(palette))
            .with_palette_overrides(self.stylesheet.resolve_font_palette_overrides(palette));
        if let Some(started) = diagnostic_started {
            let elapsed_us = started.elapsed().as_micros();
            LAYOUT_TEXT_METRICS_DIAGNOSTIC.with(|diagnostic| {
                let (calls, total_us) = diagnostic.get();
                diagnostic.set((calls.saturating_add(1), total_us.saturating_add(elapsed_us)));
            });
        }
        metrics
    }

    pub(crate) fn text_line_height_for_layout(&self, id: NativeNodeId) -> u32 {
        self.text_metrics_for_layout(id).line_height()
    }

    pub(crate) fn raw_text_for_layout(&self, id: NativeNodeId) -> Option<String> {
        self.node(id)?;
        let mut text = String::new();
        self.collect_raw_text(id, &mut text);
        Some(text)
    }

    pub(crate) fn collapse_text_for_layout(value: &str) -> (String, bool) {
        collapse_text(value, MAX_LOCATOR_BYTES)
    }
}
