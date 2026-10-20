use super::*;

impl NativeComputedStyle {
    pub(crate) const fn hidden(self) -> bool {
        matches!(self.display, DisplayValue::None) || self.visibility_hidden
    }

    pub(crate) const fn display(self) -> DisplayValue {
        self.display
    }

    pub(crate) const fn position(self) -> NativePositionValue {
        self.position
    }

    pub(crate) const fn z_index(self) -> NativeZIndexValue {
        self.z_index
    }

    pub(crate) const fn pointer_events(self) -> NativePointerEventsValue {
        self.pointer_events
    }

    pub(crate) const fn top(self) -> NativePositionOffset {
        self.top
    }

    pub(crate) const fn right(self) -> NativePositionOffset {
        self.right
    }

    pub(crate) const fn bottom(self) -> NativePositionOffset {
        self.bottom
    }

    pub(crate) const fn left(self) -> NativePositionOffset {
        self.left
    }

    pub(crate) const fn grid_template_columns(self) -> NativeGridTrackList {
        self.grid_template_columns
    }

    pub(crate) const fn grid_template_rows(self) -> NativeGridTrackList {
        self.grid_template_rows
    }

    pub(crate) const fn opacity(self) -> u8 {
        match self.opacity {
            Some(value) => value,
            None => u8::MAX,
        }
    }

    pub(crate) const fn white_space(self) -> WhiteSpaceValue {
        self.white_space
    }

    pub(crate) const fn text_align(self) -> TextAlignValue {
        self.text_align
    }

    pub(crate) const fn text_align_last(self) -> TextAlignLastValue {
        self.text_align_last
    }

    pub(crate) const fn text_justify(self) -> TextJustifyValue {
        self.text_justify
    }

    pub(crate) const fn justify_content(self) -> JustifyContentValue {
        self.justify_content
    }

    pub(crate) const fn align_items(self) -> AlignItemsValue {
        self.align_items
    }

    pub(crate) const fn align_self(self) -> AlignSelfValue {
        self.align_self
    }

    pub(crate) const fn align_content(self) -> AlignContentValue {
        self.align_content
    }

    pub(crate) const fn flex_direction(self) -> FlexDirectionValue {
        self.flex_direction
    }

    pub(crate) const fn direction(self) -> DirectionValue {
        self.direction
    }

    pub(crate) const fn flex_wrap(self) -> FlexWrapValue {
        self.flex_wrap
    }

    pub(crate) const fn flex_item_order(self) -> NativeOrderValue {
        self.flex_item_order
    }

    pub(crate) const fn flex_grow(self) -> u32 {
        self.flex_grow
    }

    pub(crate) const fn flex_shrink(self) -> u32 {
        self.flex_shrink
    }

    pub(crate) const fn flex_basis(self) -> FlexBasisValue {
        self.flex_basis
    }

    pub(crate) const fn text_decoration(self) -> TextDecorationValue {
        self.text_decoration
    }

    pub(crate) const fn text_decoration_style(self) -> NativeTextDecorationStyle {
        self.text_decoration_style
    }

    pub(crate) const fn text_decoration_skip_ink(self) -> NativeTextDecorationSkipInk {
        self.text_decoration_skip_ink
    }

    pub(crate) const fn text_decoration_skip_spaces(self) -> NativeTextDecorationSkipSpaces {
        self.text_decoration_skip_spaces
    }

    pub(crate) const fn text_decoration_thickness(self) -> u32 {
        self.text_decoration_thickness
    }

    pub(crate) const fn text_underline_offset(self) -> i32 {
        self.text_underline_offset
    }

    pub(crate) const fn text_decoration_color(self) -> Option<NativeColor> {
        self.text_decoration_color
    }

    pub(crate) const fn text_transform(self) -> TextTransformValue {
        self.text_transform
    }

    pub(crate) const fn font_variant_ligatures(self) -> NativeFontVariantLigatures {
        self.font_variant_ligatures
    }

    pub(crate) const fn font_variant_caps(self) -> NativeFontVariantCaps {
        self.font_variant_caps
    }

    pub(crate) const fn font_variant_position(self) -> NativeFontVariantPosition {
        self.font_variant_position
    }

    pub(crate) const fn font_variant_alternates(self) -> NativeFontVariantAlternates {
        self.font_variant_alternates
    }

    pub(crate) const fn font_language_override(self) -> NativeFontLanguageOverride {
        self.font_language_override
    }

    pub(crate) const fn font_variation_settings(self) -> NativeFontVariationSettings {
        self.font_variation_settings
    }

    pub(crate) const fn font_variant_east_asian(self) -> NativeFontVariantEastAsian {
        self.font_variant_east_asian
    }

    pub(crate) const fn font_variant_numeric(self) -> NativeFontVariantNumeric {
        self.font_variant_numeric
    }

    pub(crate) const fn font_feature_settings(self) -> NativeFontFeatureSettings {
        self.font_feature_settings
    }

    pub(crate) const fn font_kerning(self) -> NativeFontKerning {
        self.font_kerning
    }

    pub(crate) const fn font_optical_sizing(self) -> NativeFontOpticalSizing {
        self.font_optical_sizing
    }

    pub(crate) const fn font_palette(self) -> NativeFontPalette {
        self.font_palette
    }

    pub(crate) const fn font_weight(self) -> FontWeightValue {
        self.font_weight
    }

    pub(crate) const fn font_style(self) -> FontStyleValue {
        self.font_style
    }

    pub(crate) const fn font_stretch(self) -> NativeFontStretchRange {
        self.font_stretch
    }

    pub(crate) const fn font_family(self) -> NativeFontFamilyList {
        self.font_family
    }

    pub(crate) const fn font_size(self) -> u32 {
        self.font_size
    }

    pub(crate) const fn word_break(self) -> WordBreakValue {
        self.word_break
    }

    pub(crate) const fn text_overflow(self) -> TextOverflowValue {
        self.text_overflow
    }

    pub(crate) const fn vertical_align(self) -> VerticalAlignValue {
        self.vertical_align
    }

    pub(crate) const fn text_indent(self) -> u32 {
        self.text_indent
    }

    pub(crate) const fn word_spacing(self) -> u32 {
        self.word_spacing
    }

    pub(crate) const fn letter_spacing(self) -> u32 {
        self.letter_spacing
    }

    pub(crate) const fn column_gap(self) -> u32 {
        self.gap
    }

    pub(crate) const fn row_gap(self) -> u32 {
        self.row_gap
    }

    pub(crate) const fn width(self) -> Option<u32> {
        self.width
    }

    pub(crate) const fn height(self) -> Option<u32> {
        self.height
    }

    pub(crate) const fn min_width(self) -> Option<u32> {
        self.min_width
    }

    pub(crate) const fn max_width(self) -> Option<u32> {
        self.max_width
    }

    pub(crate) const fn min_height(self) -> Option<u32> {
        self.min_height
    }

    pub(crate) const fn max_height(self) -> Option<u32> {
        self.max_height
    }

    pub(crate) const fn line_height(self) -> Option<u32> {
        self.line_height
    }

    pub(crate) const fn background_color(self) -> Option<NativeColor> {
        self.background_color
    }

    pub(crate) const fn background_image(self) -> Option<u32> {
        self.background_image
    }

    pub(crate) const fn background_repeat(self) -> NativeBackgroundRepeat {
        self.background_repeat
    }

    pub(crate) const fn background_position(self) -> NativeBackgroundPosition {
        self.background_position
    }

    pub(crate) const fn background_size(self) -> NativeBackgroundSize {
        self.background_size
    }

    pub(crate) const fn border(self) -> Option<NativeBorder> {
        self.border
    }

    pub(crate) const fn border_colors(self) -> [NativeColor; 4] {
        match self.border_colors {
            Some(colors) => colors,
            None => [NativeColor::BLACK; 4],
        }
    }

    pub(crate) const fn border_widths(self) -> [u32; 4] {
        self.border_widths
    }

    pub(crate) const fn border_styles(self) -> [NativeBorderStyleValue; 4] {
        self.border_styles
    }

    pub(crate) const fn border_radius(self) -> NativeBorderRadius {
        self.border_radius
    }

    pub(crate) const fn padding(self) -> NativeBoxEdges {
        self.padding
    }

    pub(crate) const fn margin(self) -> NativeBoxEdges {
        self.margin
    }

    pub(crate) const fn margin_values(self) -> [NativeMarginValue; 4] {
        [
            if self.margin_auto.top() {
                NativeMarginValue::Auto
            } else {
                NativeMarginValue::Length(self.margin.top())
            },
            if self.margin_auto.right() {
                NativeMarginValue::Auto
            } else {
                NativeMarginValue::Length(self.margin.right())
            },
            if self.margin_auto.bottom() {
                NativeMarginValue::Auto
            } else {
                NativeMarginValue::Length(self.margin.bottom())
            },
            if self.margin_auto.left() {
                NativeMarginValue::Auto
            } else {
                NativeMarginValue::Length(self.margin.left())
            },
        ]
    }

    pub(crate) const fn margin_auto(self) -> NativeAutoEdges {
        self.margin_auto
    }

    pub(crate) const fn box_sizing(self) -> NativeBoxSizing {
        self.box_sizing
    }

    pub(crate) const fn is_border_box(self) -> bool {
        matches!(self.box_sizing, NativeBoxSizing::BorderBox)
    }

    pub(crate) const fn color(self) -> Option<NativeColor> {
        self.color
    }

    pub(crate) const fn overflow_clip_x(self) -> bool {
        self.overflow_clip_x
    }

    pub(crate) const fn overflow_clip_y(self) -> bool {
        self.overflow_clip_y
    }

    pub(crate) const fn overflow_x(self) -> OverflowValue {
        self.overflow_x
    }

    pub(crate) const fn overflow_y(self) -> OverflowValue {
        self.overflow_y
    }
}
