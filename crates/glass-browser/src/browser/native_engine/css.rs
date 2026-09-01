use super::config::{MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_VIEWPORT_DIMENSION};
use super::diagnostics::{NativeDiagnosticCode, NativeDiagnosticSink, NativeDiagnosticSource};
use super::dom::{NativeDocument, NativeNode, NativeNodeId};
use super::error::NativeEngineError;

pub(crate) const MAX_NATIVE_STYLE_RULES: usize = 512;
const MAX_SELECTOR_BYTES: usize = 256;
const MAX_SELECTOR_PARTS: usize = 8;

/// A bounded RGBA color used by the native display-list seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl NativeColor {
    pub const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: u8::MAX,
    };

    pub const WHITE: Self = Self {
        red: u8::MAX,
        green: u8::MAX,
        blue: u8::MAX,
        alpha: u8::MAX,
    };

    pub const RED: Self = Self {
        red: u8::MAX,
        green: 0,
        blue: 0,
        alpha: u8::MAX,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeBorderStyle {
    Solid,
    Dashed,
    Dotted,
}

/// Bounded physical circular radii for the top-left, top-right, bottom-right,
/// and bottom-left corners of one native box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeBorderRadius {
    pub top_left: u32,
    pub top_right: u32,
    pub bottom_right: u32,
    pub bottom_left: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeBorderSide {
    width: u32,
    style: NativeBorderStyle,
    color: NativeColor,
}

impl NativeBorderSide {
    pub(crate) const fn width(self) -> u32 {
        self.width
    }

    pub(crate) const fn color(self) -> NativeColor {
        self.color
    }

    pub(crate) const fn style(self) -> NativeBorderStyle {
        self.style
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeBorder {
    top: NativeBorderSide,
    right: NativeBorderSide,
    bottom: NativeBorderSide,
    left: NativeBorderSide,
}

impl NativeBorder {
    fn from_sides(sides: [Option<NativeBorderSide>; 4]) -> Option<Self> {
        let [top, right, bottom, left] = sides;
        let empty = top.is_none() && right.is_none() && bottom.is_none() && left.is_none();
        if empty {
            return None;
        }
        let zero = NativeBorderSide {
            width: 0,
            style: NativeBorderStyle::Solid,
            color: NativeColor::BLACK,
        };
        Some(Self {
            top: top.unwrap_or(zero),
            right: right.unwrap_or(zero),
            bottom: bottom.unwrap_or(zero),
            left: left.unwrap_or(zero),
        })
    }

    pub(crate) const fn top(self) -> NativeBorderSide {
        self.top
    }

    pub(crate) const fn right(self) -> NativeBorderSide {
        self.right
    }

    pub(crate) const fn bottom(self) -> NativeBorderSide {
        self.bottom
    }

    pub(crate) const fn left(self) -> NativeBorderSide {
        self.left
    }

    pub(crate) const fn any_width(self) -> bool {
        self.top.width > 0 || self.right.width > 0 || self.bottom.width > 0 || self.left.width > 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum NativeBoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum DisplayValue {
    #[default]
    Auto,
    None,
    Block,
    Inline,
    Contents,
    Flex,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WhiteSpaceValue {
    #[default]
    Normal,
    PreLine,
    Pre,
    PreWrap,
    NoWrap,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextAlignValue {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum JustifyContentValue {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextDecorationValue {
    #[default]
    None,
    Underline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextTransformValue {
    #[default]
    None,
    Uppercase,
    Lowercase,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FontWeightValue {
    #[default]
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FontStyleValue {
    #[default]
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WordBreakValue {
    #[default]
    Normal,
    BreakAll,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextOverflowValue {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum VerticalAlignValue {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeInheritedStyle {
    pub(crate) color: Option<NativeColor>,
    pub(crate) white_space: WhiteSpaceValue,
    pub(crate) line_height: Option<u32>,
    pub(crate) text_align: TextAlignValue,
    pub(crate) text_decoration: TextDecorationValue,
    pub(crate) text_transform: TextTransformValue,
    pub(crate) font_weight: FontWeightValue,
    pub(crate) font_style: FontStyleValue,
    pub(crate) word_break: WordBreakValue,
    pub(crate) vertical_align: VerticalAlignValue,
    pub(crate) word_spacing: u32,
    pub(crate) letter_spacing: u32,
}

/// Bounded physical top, right, bottom, and left box values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeBoxEdges {
    top: u32,
    right: u32,
    bottom: u32,
    left: u32,
}

impl NativeBoxEdges {
    fn from_cascade(values: [Option<CascadeValue<u32>>; 4]) -> Self {
        Self {
            top: values[0].map_or(0, |value| value.value),
            right: values[1].map_or(0, |value| value.value),
            bottom: values[2].map_or(0, |value| value.value),
            left: values[3].map_or(0, |value| value.value),
        }
    }

    pub(crate) const fn top(self) -> u32 {
        self.top
    }

    pub(crate) const fn right(self) -> u32 {
        self.right
    }

    pub(crate) const fn bottom(self) -> u32 {
        self.bottom
    }

    pub(crate) const fn left(self) -> u32 {
        self.left
    }

    pub(crate) const fn horizontal(self) -> u32 {
        self.left.saturating_add(self.right)
    }

    pub(crate) const fn vertical(self) -> u32 {
        self.top.saturating_add(self.bottom)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeComputedStyle {
    display: DisplayValue,
    visibility_hidden: bool,
    opacity: Option<u8>,
    white_space: WhiteSpaceValue,
    text_align: TextAlignValue,
    justify_content: JustifyContentValue,
    text_decoration: TextDecorationValue,
    text_transform: TextTransformValue,
    font_weight: FontWeightValue,
    font_style: FontStyleValue,
    word_break: WordBreakValue,
    text_overflow: TextOverflowValue,
    vertical_align: VerticalAlignValue,
    text_indent: u32,
    word_spacing: u32,
    letter_spacing: u32,
    gap: u32,
    width: Option<u32>,
    height: Option<u32>,
    min_width: Option<u32>,
    max_width: Option<u32>,
    min_height: Option<u32>,
    max_height: Option<u32>,
    line_height: Option<u32>,
    background_color: Option<NativeColor>,
    border: Option<NativeBorder>,
    border_radius: NativeBorderRadius,
    padding: NativeBoxEdges,
    margin: NativeBoxEdges,
    box_sizing: NativeBoxSizing,
    color: Option<NativeColor>,
    overflow_clip_x: bool,
    overflow_clip_y: bool,
}

impl NativeComputedStyle {
    pub(crate) const fn hidden(self) -> bool {
        matches!(self.display, DisplayValue::None) || self.visibility_hidden
    }

    pub(crate) const fn display(self) -> DisplayValue {
        self.display
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

    pub(crate) const fn justify_content(self) -> JustifyContentValue {
        self.justify_content
    }

    pub(crate) const fn text_decoration(self) -> TextDecorationValue {
        self.text_decoration
    }

    pub(crate) const fn text_transform(self) -> TextTransformValue {
        self.text_transform
    }

    pub(crate) const fn font_weight(self) -> FontWeightValue {
        self.font_weight
    }

    pub(crate) const fn font_style(self) -> FontStyleValue {
        self.font_style
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

    pub(crate) const fn gap(self) -> u32 {
        self.gap
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

    pub(crate) const fn border(self) -> Option<NativeBorder> {
        self.border
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NativeStylesheet {
    rules: Vec<NativeStyleRule>,
}

impl NativeStylesheet {
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
        for (index, source) in sources.into_iter().enumerate() {
            parse_source(
                &source,
                &mut stylesheet.rules,
                &mut order,
                NativeDiagnosticSource::Stylesheet { index },
                diagnostics,
            )?;
        }
        Ok(stylesheet)
    }

    #[cfg(test)]
    pub(crate) fn computed_for(&self, node: &NativeNode) -> NativeComputedStyle {
        self.computed_for_with_matcher(node, NativeInheritedStyle::default(), |selector| {
            selector.matches(node)
        })
    }

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
                ..NativeInheritedStyle::default()
            },
        )
    }

    pub(crate) fn computed_for_in_document_with_inheritance(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        inherited: NativeInheritedStyle,
    ) -> NativeComputedStyle {
        let Some(node) = document.node(node_id) else {
            return NativeComputedStyle::default();
        };
        self.computed_for_with_matcher(node, inherited, |selector| {
            selector.matches_in_document(document, node_id)
        })
    }

    fn computed_for_with_matcher(
        &self,
        node: &NativeNode,
        inherited: NativeInheritedStyle,
        matches: impl Fn(&NativeSelector) -> bool,
    ) -> NativeComputedStyle {
        let mut display = None;
        let mut visibility = None;
        let mut opacity = None;
        let mut white_space = None;
        let mut text_align = None;
        let mut justify_content = None;
        let mut text_decoration = None;
        let mut text_transform = None;
        let mut font_weight = None;
        let mut font_style = None;
        let mut word_break = None;
        let mut text_overflow = None;
        let mut vertical_align = None;
        let mut text_indent = None;
        let mut word_spacing = None;
        let mut letter_spacing = None;
        let mut gap = None;
        let mut width = None;
        let mut height = None;
        let mut min_width = None;
        let mut max_width = None;
        let mut min_height = None;
        let mut max_height = None;
        let mut line_height = None;
        let mut background_color = None;
        let mut border: [Option<CascadeValue<NativeBorderSide>>; 4] = [None; 4];
        let mut border_radius = None;
        let mut padding: [Option<CascadeValue<u32>>; 4] = [None; 4];
        let mut margin: [Option<CascadeValue<u32>>; 4] = [None; 4];
        let mut box_sizing = None;
        let mut color = None;
        let mut overflow_x = None;
        let mut overflow_y = None;
        for rule in &self.rules {
            if !matches(&rule.selector) {
                continue;
            }
            if let Some(value) = rule.declarations.display
                && wins(rule.selector.specificity, rule.order, false, display)
            {
                display = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.visibility
                && wins(rule.selector.specificity, rule.order, false, visibility)
            {
                visibility = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.opacity
                && wins(rule.selector.specificity, rule.order, false, opacity)
            {
                opacity = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.white_space
                && wins(rule.selector.specificity, rule.order, false, white_space)
            {
                white_space = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_align
                && wins(rule.selector.specificity, rule.order, false, text_align)
            {
                text_align = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_decoration
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration,
                )
            {
                text_decoration = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_transform
                && wins(rule.selector.specificity, rule.order, false, text_transform)
            {
                text_transform = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.font_weight
                && wins(rule.selector.specificity, rule.order, false, font_weight)
            {
                font_weight = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.font_style
                && wins(rule.selector.specificity, rule.order, false, font_style)
            {
                font_style = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.word_break
                && wins(rule.selector.specificity, rule.order, false, word_break)
            {
                word_break = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_overflow
                && wins(rule.selector.specificity, rule.order, false, text_overflow)
            {
                text_overflow = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.vertical_align
                && wins(rule.selector.specificity, rule.order, false, vertical_align)
            {
                vertical_align = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_indent
                && wins(rule.selector.specificity, rule.order, false, text_indent)
            {
                text_indent = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.word_spacing
                && wins(rule.selector.specificity, rule.order, false, word_spacing)
            {
                word_spacing = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.letter_spacing
                && wins(rule.selector.specificity, rule.order, false, letter_spacing)
            {
                letter_spacing = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.gap
                && wins(rule.selector.specificity, rule.order, false, gap)
            {
                gap = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.justify_content
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    justify_content,
                )
            {
                justify_content = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.width
                && wins(rule.selector.specificity, rule.order, false, width)
            {
                width = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.height
                && wins(rule.selector.specificity, rule.order, false, height)
            {
                height = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.min_width
                && wins(rule.selector.specificity, rule.order, false, min_width)
            {
                min_width = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.max_width
                && wins(rule.selector.specificity, rule.order, false, max_width)
            {
                max_width = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.min_height
                && wins(rule.selector.specificity, rule.order, false, min_height)
            {
                min_height = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.max_height
                && wins(rule.selector.specificity, rule.order, false, max_height)
            {
                max_height = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.line_height
                && wins(rule.selector.specificity, rule.order, false, line_height)
            {
                line_height = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.background_color
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    background_color,
                )
            {
                background_color = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            apply_border_sides(
                &rule.declarations.border,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border,
            );
            if let Some(value) = rule.declarations.border_radius
                && wins(rule.selector.specificity, rule.order, false, border_radius)
            {
                border_radius = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            apply_box_edges(
                &rule.declarations.padding,
                rule.selector.specificity,
                rule.order,
                false,
                &mut padding,
            );
            apply_box_edges(
                &rule.declarations.margin,
                rule.selector.specificity,
                rule.order,
                false,
                &mut margin,
            );
            if let Some(value) = rule.declarations.box_sizing
                && wins(rule.selector.specificity, rule.order, false, box_sizing)
            {
                box_sizing = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.color
                && wins(rule.selector.specificity, rule.order, false, color)
            {
                color = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.overflow_x
                && wins(rule.selector.specificity, rule.order, false, overflow_x)
            {
                overflow_x = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.overflow_y
                && wins(rule.selector.specificity, rule.order, false, overflow_y)
            {
                overflow_y = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
        }

        if let Some(inline_style) = node.attribute("style") {
            let declarations = parse_declarations(inline_style);
            if let Some(value) = declarations.display
                && wins(u16::MAX, usize::MAX, true, display)
            {
                display = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.visibility
                && wins(u16::MAX, usize::MAX, true, visibility)
            {
                visibility = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.opacity
                && wins(u16::MAX, usize::MAX, true, opacity)
            {
                opacity = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.white_space
                && wins(u16::MAX, usize::MAX, true, white_space)
            {
                white_space = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_align
                && wins(u16::MAX, usize::MAX, true, text_align)
            {
                text_align = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_decoration
                && wins(u16::MAX, usize::MAX, true, text_decoration)
            {
                text_decoration = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_transform
                && wins(u16::MAX, usize::MAX, true, text_transform)
            {
                text_transform = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.font_weight
                && wins(u16::MAX, usize::MAX, true, font_weight)
            {
                font_weight = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.font_style
                && wins(u16::MAX, usize::MAX, true, font_style)
            {
                font_style = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.word_break
                && wins(u16::MAX, usize::MAX, true, word_break)
            {
                word_break = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_overflow
                && wins(u16::MAX, usize::MAX, true, text_overflow)
            {
                text_overflow = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.vertical_align
                && wins(u16::MAX, usize::MAX, true, vertical_align)
            {
                vertical_align = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_indent
                && wins(u16::MAX, usize::MAX, true, text_indent)
            {
                text_indent = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.word_spacing
                && wins(u16::MAX, usize::MAX, true, word_spacing)
            {
                word_spacing = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.letter_spacing
                && wins(u16::MAX, usize::MAX, true, letter_spacing)
            {
                letter_spacing = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.gap
                && wins(u16::MAX, usize::MAX, true, gap)
            {
                gap = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.justify_content
                && wins(u16::MAX, usize::MAX, true, justify_content)
            {
                justify_content = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.width
                && wins(u16::MAX, usize::MAX, true, width)
            {
                width = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.height
                && wins(u16::MAX, usize::MAX, true, height)
            {
                height = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.min_width
                && wins(u16::MAX, usize::MAX, true, min_width)
            {
                min_width = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.max_width
                && wins(u16::MAX, usize::MAX, true, max_width)
            {
                max_width = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.min_height
                && wins(u16::MAX, usize::MAX, true, min_height)
            {
                min_height = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.max_height
                && wins(u16::MAX, usize::MAX, true, max_height)
            {
                max_height = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.line_height
                && wins(u16::MAX, usize::MAX, true, line_height)
            {
                line_height = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.background_color
                && wins(u16::MAX, usize::MAX, true, background_color)
            {
                background_color = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            apply_border_sides(
                &declarations.border,
                u16::MAX,
                usize::MAX,
                true,
                &mut border,
            );
            if let Some(value) = declarations.border_radius
                && wins(u16::MAX, usize::MAX, true, border_radius)
            {
                border_radius = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            apply_box_edges(
                &declarations.padding,
                u16::MAX,
                usize::MAX,
                true,
                &mut padding,
            );
            apply_box_edges(
                &declarations.margin,
                u16::MAX,
                usize::MAX,
                true,
                &mut margin,
            );
            if let Some(value) = declarations.box_sizing
                && wins(u16::MAX, usize::MAX, true, box_sizing)
            {
                box_sizing = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.color
                && wins(u16::MAX, usize::MAX, true, color)
            {
                color = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.overflow_x
                && wins(u16::MAX, usize::MAX, true, overflow_x)
            {
                overflow_x = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.overflow_y
                && wins(u16::MAX, usize::MAX, true, overflow_y)
            {
                overflow_y = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
        }

        NativeComputedStyle {
            display: display.map_or(DisplayValue::Auto, |value| value.value),
            visibility_hidden: visibility
                .is_some_and(|value| value.value == VisibilityValue::Hidden),
            opacity: opacity.map(|value| value.value),
            white_space: white_space.map_or(inherited.white_space, |value| value.value),
            text_align: text_align.map_or(inherited.text_align, |value| value.value),
            justify_content: justify_content
                .map_or(JustifyContentValue::FlexStart, |value| value.value),
            text_decoration: text_decoration.map_or(inherited.text_decoration, |value| value.value),
            text_transform: text_transform.map_or(inherited.text_transform, |value| value.value),
            font_weight: font_weight.map_or(inherited.font_weight, |value| value.value),
            font_style: font_style.map_or(inherited.font_style, |value| value.value),
            word_break: word_break.map_or(inherited.word_break, |value| value.value),
            text_overflow: text_overflow.map_or(TextOverflowValue::Clip, |value| value.value),
            vertical_align: vertical_align.map_or(inherited.vertical_align, |value| value.value),
            text_indent: text_indent.map_or(0, |value| value.value),
            word_spacing: word_spacing.map_or(inherited.word_spacing, |value| value.value),
            letter_spacing: letter_spacing.map_or(inherited.letter_spacing, |value| value.value),
            gap: gap.map_or(0, |value| value.value),
            width: width.map(|value| value.value),
            height: height.map(|value| value.value),
            min_width: min_width.map(|value| value.value),
            max_width: max_width.map(|value| value.value),
            min_height: min_height.map(|value| value.value),
            max_height: max_height.map(|value| value.value),
            line_height: line_height
                .map(|value| value.value)
                .or(inherited.line_height),
            background_color: background_color.map(|value| value.value),
            border: NativeBorder::from_sides(border.map(|value| value.map(|value| value.value))),
            border_radius: border_radius.map_or(NativeBorderRadius::default(), |value| value.value),
            padding: NativeBoxEdges::from_cascade(padding),
            margin: NativeBoxEdges::from_cascade(margin),
            box_sizing: box_sizing.map_or(NativeBoxSizing::ContentBox, |value| value.value),
            color: color.map(|value| value.value).or(inherited.color),
            overflow_clip_x: overflow_x.is_some_and(|value| {
                matches!(value.value, OverflowValue::Hidden | OverflowValue::Clip)
            }),
            overflow_clip_y: overflow_y.is_some_and(|value| {
                matches!(value.value, OverflowValue::Hidden | OverflowValue::Clip)
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityValue {
    Hidden,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverflowValue {
    Hidden,
    Clip,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CascadeValue<T> {
    value: T,
    specificity: u16,
    order: usize,
    inline: bool,
}

fn wins<T>(specificity: u16, order: usize, inline: bool, current: Option<CascadeValue<T>>) -> bool {
    current.is_none_or(|current| {
        (inline, specificity, order) > (current.inline, current.specificity, current.order)
    })
}

fn apply_border_sides(
    declarations: &[Option<NativeBorderSide>; 4],
    specificity: u16,
    order: usize,
    inline: bool,
    border: &mut [Option<CascadeValue<NativeBorderSide>>; 4],
) {
    for (index, value) in declarations.iter().enumerate() {
        if let Some(value) = value
            && wins(specificity, order, inline, border[index])
        {
            border[index] = Some(CascadeValue {
                value: *value,
                specificity,
                order,
                inline,
            });
        }
    }
}

fn apply_box_edges(
    declarations: &[Option<u32>; 4],
    specificity: u16,
    order: usize,
    inline: bool,
    edges: &mut [Option<CascadeValue<u32>>; 4],
) {
    for (index, value) in declarations.iter().enumerate() {
        if let Some(value) = value
            && wins(specificity, order, inline, edges[index])
        {
            edges[index] = Some(CascadeValue {
                value: *value,
                specificity,
                order,
                inline,
            });
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeDeclarations {
    display: Option<DisplayValue>,
    visibility: Option<VisibilityValue>,
    opacity: Option<u8>,
    white_space: Option<WhiteSpaceValue>,
    text_align: Option<TextAlignValue>,
    justify_content: Option<JustifyContentValue>,
    text_decoration: Option<TextDecorationValue>,
    text_transform: Option<TextTransformValue>,
    font_weight: Option<FontWeightValue>,
    font_style: Option<FontStyleValue>,
    word_break: Option<WordBreakValue>,
    text_overflow: Option<TextOverflowValue>,
    vertical_align: Option<VerticalAlignValue>,
    text_indent: Option<u32>,
    word_spacing: Option<u32>,
    letter_spacing: Option<u32>,
    gap: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    min_width: Option<u32>,
    max_width: Option<u32>,
    min_height: Option<u32>,
    max_height: Option<u32>,
    line_height: Option<u32>,
    background_color: Option<NativeColor>,
    border: [Option<NativeBorderSide>; 4],
    border_radius: Option<NativeBorderRadius>,
    padding: [Option<u32>; 4],
    margin: [Option<u32>; 4],
    box_sizing: Option<NativeBoxSizing>,
    color: Option<NativeColor>,
    overflow: Option<OverflowValue>,
    overflow_x: Option<OverflowValue>,
    overflow_y: Option<OverflowValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeStyleRule {
    selector: NativeSelector,
    declarations: NativeDeclarations,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeSelector {
    compounds: Vec<NativeCompoundSelector>,
    specificity: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeAttributeSelector {
    name: String,
    value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCompoundSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<NativeAttributeSelector>,
    specificity: u16,
}

impl NativeSelector {
    #[cfg(test)]
    fn matches(&self, node: &NativeNode) -> bool {
        self.compounds
            .last()
            .is_some_and(|compound| compound.matches(node))
    }

    fn matches_in_document(&self, document: &NativeDocument, node_id: NativeNodeId) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };
        if node.element_name().is_none() {
            return false;
        }
        let Some(target) = self.compounds.last() else {
            return false;
        };
        if !target.matches(node) {
            return false;
        }

        let mut ancestor = node.parent();
        for compound in self.compounds.iter().rev().skip(1) {
            let mut found = false;
            for _ in 0..=MAX_NATIVE_DOM_DEPTH {
                let Some(ancestor_id) = ancestor else {
                    break;
                };
                let Some(ancestor_node) = document.node(ancestor_id) else {
                    break;
                };
                ancestor = ancestor_node.parent();
                if ancestor_node.element_name().is_some() && compound.matches(ancestor_node) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }
        true
    }
}

impl NativeCompoundSelector {
    fn matches(&self, node: &NativeNode) -> bool {
        if self
            .tag
            .as_deref()
            .is_some_and(|tag| node.element_name() != Some(tag))
        {
            return false;
        }
        if self
            .id
            .as_deref()
            .is_some_and(|id| node.attribute("id") != Some(id))
        {
            return false;
        }
        if self.classes.iter().any(|class| {
            !node
                .attribute("class")
                .is_some_and(|value| value.split_ascii_whitespace().any(|item| item == class))
        }) {
            return false;
        }
        self.attributes.iter().all(|attribute| {
            node.attribute(&attribute.name).is_some_and(|value| {
                attribute
                    .value
                    .as_deref()
                    .is_none_or(|expected| value == expected)
            })
        })
    }
}

fn parse_source(
    source: &str,
    rules: &mut Vec<NativeStyleRule>,
    next_order: &mut usize,
    diagnostic_source: NativeDiagnosticSource,
    diagnostics: &mut NativeDiagnosticSink,
) -> Result<(), NativeEngineError> {
    let source = strip_comments(source);
    let mut cursor = 0;
    let mut unclosed_rule = false;
    while let Some(open_relative) = source[cursor..].find('{') {
        let open = cursor + open_relative;
        let Some(close_relative) = source[open + 1..].find('}') else {
            diagnostics.push(
                NativeDiagnosticCode::MalformedCss,
                diagnostic_source,
                open,
                "unclosed-rule",
            );
            unclosed_rule = true;
            break;
        };
        let close = open + 1 + close_relative;
        let declarations = parse_declarations_with_diagnostics(
            &source[open + 1..close],
            diagnostic_source,
            open.saturating_add(1),
            diagnostics,
        );
        let has_supported_declaration = declarations.display.is_some()
            || declarations.visibility.is_some()
            || declarations.opacity.is_some()
            || declarations.white_space.is_some()
            || declarations.text_align.is_some()
            || declarations.justify_content.is_some()
            || declarations.text_decoration.is_some()
            || declarations.text_transform.is_some()
            || declarations.font_weight.is_some()
            || declarations.font_style.is_some()
            || declarations.word_break.is_some()
            || declarations.text_overflow.is_some()
            || declarations.vertical_align.is_some()
            || declarations.text_indent.is_some()
            || declarations.word_spacing.is_some()
            || declarations.letter_spacing.is_some()
            || declarations.gap.is_some()
            || declarations.width.is_some()
            || declarations.height.is_some()
            || declarations.min_width.is_some()
            || declarations.max_width.is_some()
            || declarations.min_height.is_some()
            || declarations.max_height.is_some()
            || declarations.line_height.is_some()
            || declarations.background_color.is_some()
            || declarations.border.iter().any(Option::is_some)
            || declarations.border_radius.is_some()
            || declarations.padding.iter().any(Option::is_some)
            || declarations.margin.iter().any(Option::is_some)
            || declarations.box_sizing.is_some()
            || declarations.color.is_some()
            || declarations.overflow.is_some()
            || declarations.overflow_x.is_some()
            || declarations.overflow_y.is_some();
        let selector_source = &source[cursor..open];
        let mut selector_offset = cursor;
        for selector_text in selector_source.split(',') {
            let Some(selector) = parse_selector(selector_text) else {
                diagnostics.push(
                    NativeDiagnosticCode::UnsupportedCssSelector,
                    diagnostic_source,
                    selector_offset.saturating_add(
                        selector_text
                            .len()
                            .saturating_sub(selector_text.trim_start().len()),
                    ),
                    selector_diagnostic_detail(selector_text),
                );
                selector_offset = selector_offset.saturating_add(selector_text.len() + 1);
                continue;
            };
            if has_supported_declaration {
                if rules.len() >= MAX_NATIVE_STYLE_RULES {
                    return Err(NativeEngineError::limit(
                        "CSS style rules",
                        MAX_NATIVE_STYLE_RULES,
                        rules.len().saturating_add(1),
                    ));
                }
                rules.push(NativeStyleRule {
                    selector,
                    declarations,
                    order: *next_order,
                });
                *next_order = next_order.saturating_add(1);
            }
            selector_offset = selector_offset.saturating_add(selector_text.len() + 1);
        }
        cursor = close + 1;
    }
    if !unclosed_rule && !source[cursor..].trim().is_empty() {
        diagnostics.push(
            NativeDiagnosticCode::MalformedCss,
            diagnostic_source,
            cursor,
            "missing-rule",
        );
    }
    Ok(())
}

fn parse_declarations_with_diagnostics(
    source: &str,
    diagnostic_source: NativeDiagnosticSource,
    base_offset: usize,
    diagnostics: &mut NativeDiagnosticSink,
) -> NativeDeclarations {
    let declarations = parse_declarations(source);
    let mut declaration_offset = 0;
    for declaration in source.split(';') {
        let offset = base_offset.saturating_add(declaration_offset);
        declaration_offset = declaration_offset.saturating_add(declaration.len().saturating_add(1));
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
        let value = value.strip_suffix("!important").map_or(value, str::trim);
        let property_name = property.to_ascii_lowercase();
        let supported = match property_name.as_str() {
            "display" => {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "none"
                        | "block"
                        | "flow-root"
                        | "list-item"
                        | "table"
                        | "inline"
                        | "inline-block"
                        | "inline-flex"
                        | "inline-grid"
                        | "flex"
                        | "contents"
                )
            }
            "visibility" => parse_visibility(value).is_some(),
            "opacity" => parse_opacity(value).is_some(),
            "white-space" => parse_white_space(value).is_some(),
            "text-align" => parse_text_align(value).is_some(),
            "justify-content" => parse_justify_content(value).is_some(),
            "text-decoration" => parse_text_decoration(value).is_some(),
            "text-transform" => parse_text_transform(value).is_some(),
            "font-weight" => parse_font_weight(value).is_some(),
            "font-style" => parse_font_style(value).is_some(),
            "word-break" => parse_word_break(value).is_some(),
            "text-overflow" => parse_text_overflow(value).is_some(),
            "vertical-align" => parse_vertical_align(value).is_some(),
            "text-indent" => parse_dimension(value).is_some(),
            "word-spacing" => parse_dimension(value).is_some(),
            "letter-spacing" => parse_dimension(value).is_some(),
            "gap" => parse_dimension(value).is_some(),
            "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height" => {
                parse_dimension(value).is_some()
            }
            "line-height" => parse_line_height(value).is_some(),
            "background-color" | "color" => parse_color(value).is_some(),
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                parse_border(value).is_some()
            }
            "border-radius" => parse_border_radius(value).is_some(),
            "padding" | "margin" => parse_box_edges(value).is_some(),
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" | "margin-top"
            | "margin-right" | "margin-bottom" | "margin-left" => parse_dimension(value).is_some(),
            "box-sizing" => parse_box_sizing(value).is_some(),
            "overflow" | "overflow-x" | "overflow-y" => parse_overflow(value)
                .is_some_and(|value| matches!(value, OverflowValue::Hidden | OverflowValue::Clip)),
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

fn is_known_css_property(property: &str) -> bool {
    matches!(
        property,
        "display"
            | "visibility"
            | "opacity"
            | "white-space"
            | "text-align"
            | "justify-content"
            | "text-decoration"
            | "text-transform"
            | "font-weight"
            | "font-style"
            | "word-break"
            | "text-overflow"
            | "vertical-align"
            | "text-indent"
            | "gap"
            | "width"
            | "height"
            | "min-width"
            | "max-width"
            | "min-height"
            | "max-height"
            | "line-height"
            | "background-color"
            | "color"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-radius"
            | "padding"
            | "margin"
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

fn selector_diagnostic_detail(selector: &str) -> &'static str {
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

fn parse_declarations(source: &str) -> NativeDeclarations {
    let mut declarations = NativeDeclarations::default();
    for declaration in source.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let value = value.strip_suffix("!important").map_or(value, str::trim);
        match property.to_ascii_lowercase().as_str() {
            "display" => {
                declarations.display = parse_display(value);
            }
            "visibility" => {
                declarations.visibility = parse_visibility(value);
            }
            "opacity" => {
                declarations.opacity = parse_opacity(value);
            }
            "white-space" => {
                declarations.white_space = parse_white_space(value);
            }
            "text-align" => {
                declarations.text_align = parse_text_align(value);
            }
            "justify-content" => {
                declarations.justify_content = parse_justify_content(value);
            }
            "text-decoration" => {
                declarations.text_decoration = parse_text_decoration(value);
            }
            "text-transform" => {
                declarations.text_transform = parse_text_transform(value);
            }
            "font-weight" => {
                declarations.font_weight = parse_font_weight(value);
            }
            "font-style" => {
                declarations.font_style = parse_font_style(value);
            }
            "word-break" => {
                declarations.word_break = parse_word_break(value);
            }
            "text-overflow" => {
                declarations.text_overflow = parse_text_overflow(value);
            }
            "vertical-align" => {
                declarations.vertical_align = parse_vertical_align(value);
            }
            "text-indent" => {
                declarations.text_indent = parse_dimension(value);
            }
            "word-spacing" => {
                declarations.word_spacing = parse_dimension(value);
            }
            "letter-spacing" => {
                declarations.letter_spacing = parse_dimension(value);
            }
            "gap" => {
                declarations.gap = parse_dimension(value);
            }
            "width" => {
                declarations.width = parse_dimension(value);
            }
            "height" => {
                declarations.height = parse_dimension(value);
            }
            "min-width" => {
                declarations.min_width = parse_dimension(value);
            }
            "max-width" => {
                declarations.max_width = parse_dimension(value);
            }
            "min-height" => {
                declarations.min_height = parse_dimension(value);
            }
            "max-height" => {
                declarations.max_height = parse_dimension(value);
            }
            "line-height" => {
                declarations.line_height = parse_line_height(value);
            }
            "background-color" => {
                declarations.background_color = parse_color(value);
            }
            "border" => {
                if let Some(border) = parse_border(value) {
                    declarations.border = [Some(border); 4];
                }
            }
            "border-top" => {
                set_border_side(&mut declarations.border, 0, value);
            }
            "border-right" => {
                set_border_side(&mut declarations.border, 1, value);
            }
            "border-bottom" => {
                set_border_side(&mut declarations.border, 2, value);
            }
            "border-left" => {
                set_border_side(&mut declarations.border, 3, value);
            }
            "border-radius" => {
                declarations.border_radius = parse_border_radius(value);
            }
            "padding" => {
                if let Some(values) = parse_box_edges(value) {
                    declarations.padding = values.map(Some);
                }
            }
            "margin" => {
                if let Some(values) = parse_box_edges(value) {
                    declarations.margin = values.map(Some);
                }
            }
            "padding-top" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.padding[0] = Some(value);
                }
            }
            "padding-right" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.padding[1] = Some(value);
                }
            }
            "padding-bottom" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.padding[2] = Some(value);
                }
            }
            "padding-left" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.padding[3] = Some(value);
                }
            }
            "margin-top" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.margin[0] = Some(value);
                }
            }
            "margin-right" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.margin[1] = Some(value);
                }
            }
            "margin-bottom" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.margin[2] = Some(value);
                }
            }
            "margin-left" => {
                if let Some(value) = parse_dimension(value) {
                    declarations.margin[3] = Some(value);
                }
            }
            "box-sizing" => {
                declarations.box_sizing = parse_box_sizing(value);
            }
            "color" => {
                declarations.color = parse_color(value);
            }
            "overflow" => {
                let parsed = parse_overflow(value);
                declarations.overflow = parsed;
                declarations.overflow_x = parsed;
                declarations.overflow_y = parsed;
            }
            "overflow-x" => {
                declarations.overflow_x = parse_overflow(value);
            }
            "overflow-y" => {
                declarations.overflow_y = parse_overflow(value);
            }
            _ => {}
        }
    }
    declarations
}

fn parse_border(value: &str) -> Option<NativeBorderSide> {
    let mut parts = value.split_ascii_whitespace();
    let width = parse_dimension(parts.next()?)?;
    let style = parse_border_style(parts.next()?)?;
    let color = parts.collect::<Vec<_>>().join(" ");
    Some(NativeBorderSide {
        width,
        style,
        color: parse_color(&color)?,
    })
}

fn parse_border_radius(value: &str) -> Option<NativeBorderRadius> {
    if value.contains('/') {
        return None;
    }
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some(NativeBorderRadius {
            top_left: *all,
            top_right: *all,
            bottom_right: *all,
            bottom_left: *all,
        }),
        [top_left, top_right] => Some(NativeBorderRadius {
            top_left: *top_left,
            top_right: *top_right,
            bottom_right: *top_left,
            bottom_left: *top_right,
        }),
        [top_left, top_right, bottom_right] => Some(NativeBorderRadius {
            top_left: *top_left,
            top_right: *top_right,
            bottom_right: *bottom_right,
            bottom_left: *top_right,
        }),
        [top_left, top_right, bottom_right, bottom_left] => Some(NativeBorderRadius {
            top_left: *top_left,
            top_right: *top_right,
            bottom_right: *bottom_right,
            bottom_left: *bottom_left,
        }),
        _ => None,
    }
}

fn parse_box_edges(value: &str) -> Option<[u32; 4]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some([*all; 4]),
        [vertical, horizontal] => Some([*vertical, *horizontal, *vertical, *horizontal]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

fn parse_border_style(value: &str) -> Option<NativeBorderStyle> {
    match value.to_ascii_lowercase().as_str() {
        "solid" => Some(NativeBorderStyle::Solid),
        "dashed" => Some(NativeBorderStyle::Dashed),
        "dotted" => Some(NativeBorderStyle::Dotted),
        _ => None,
    }
}

fn set_border_side(sides: &mut [Option<NativeBorderSide>; 4], index: usize, value: &str) {
    if let Some(border) = parse_border(value) {
        sides[index] = Some(border);
    }
}

fn parse_box_sizing(value: &str) -> Option<NativeBoxSizing> {
    match value.to_ascii_lowercase().as_str() {
        "content-box" => Some(NativeBoxSizing::ContentBox),
        "border-box" => Some(NativeBoxSizing::BorderBox),
        _ => None,
    }
}

fn parse_color(value: &str) -> Option<NativeColor> {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "black" => Some(NativeColor::BLACK),
        "white" => Some(NativeColor::WHITE),
        "red" => Some(NativeColor::RED),
        "green" => Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        }),
        "blue" => Some(NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        }),
        "transparent" => Some(NativeColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        }),
        _ if value.starts_with('#') => parse_hex_color(&value[1..]),
        _ if value.starts_with("rgb(") && value.ends_with(')') => {
            let values = value[4..value.len() - 1]
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            if values.len() != 3 {
                return None;
            }
            Some(NativeColor {
                red: values[0].parse().ok()?,
                green: values[1].parse().ok()?,
                blue: values[2].parse().ok()?,
                alpha: u8::MAX,
            })
        }
        _ if value.starts_with("rgba(") && value.ends_with(')') => {
            let values = value[5..value.len() - 1]
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>();
            if values.len() != 4 {
                return None;
            }
            Some(NativeColor {
                red: parse_color_channel(values[0])?,
                green: parse_color_channel(values[1])?,
                blue: parse_color_channel(values[2])?,
                alpha: parse_opacity(values[3])?,
            })
        }
        _ => None,
    }
}

fn parse_color_channel(value: &str) -> Option<u8> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn parse_hex_color(value: &str) -> Option<NativeColor> {
    if !value.is_ascii() {
        return None;
    }
    let component = |value: &str| u8::from_str_radix(value, 16).ok();
    match value.len() {
        3 => Some(NativeColor {
            red: component(&value[0..1])?.saturating_mul(17),
            green: component(&value[1..2])?.saturating_mul(17),
            blue: component(&value[2..3])?.saturating_mul(17),
            alpha: u8::MAX,
        }),
        6 => Some(NativeColor {
            red: component(&value[0..2])?,
            green: component(&value[2..4])?,
            blue: component(&value[4..6])?,
            alpha: u8::MAX,
        }),
        8 => Some(NativeColor {
            red: component(&value[0..2])?,
            green: component(&value[2..4])?,
            blue: component(&value[4..6])?,
            alpha: component(&value[6..8])?,
        }),
        _ => None,
    }
}

fn parse_display(value: &str) -> Option<DisplayValue> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(DisplayValue::None),
        "block" | "flow-root" | "list-item" | "table" => Some(DisplayValue::Block),
        "inline" | "inline-block" | "inline-flex" | "inline-grid" => Some(DisplayValue::Inline),
        "contents" => Some(DisplayValue::Contents),
        "flex" => Some(DisplayValue::Flex),
        "grid" => Some(DisplayValue::Other),
        _ => None,
    }
}

fn parse_dimension(value: &str) -> Option<u32> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_suffix("px")?.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = value.parse::<u32>().ok()?;
    (value <= MAX_NATIVE_VIEWPORT_DIMENSION).then_some(value)
}

fn parse_line_height(value: &str) -> Option<u32> {
    parse_dimension(value).filter(|value| *value > 0)
}

fn parse_opacity(value: &str) -> Option<u8> {
    let value = value.trim();
    let percentage = value.strip_suffix('%');
    let number = percentage.unwrap_or(value);
    let scaled = parse_decimal_milli(number)?;
    let denominator: u32 = if percentage.is_some() { 100_000 } else { 1_000 };
    if scaled > denominator {
        return None;
    }
    let alpha = (u64::from(scaled) * u64::from(u8::MAX) + u64::from(denominator / 2))
        / u64::from(denominator);
    u8::try_from(alpha).ok()
}

fn parse_decimal_milli(value: &str) -> Option<u32> {
    let (whole, fraction, has_decimal) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, fraction, true),
        None => (value, "", false),
    };
    if whole.is_empty() && !has_decimal || has_decimal && fraction.is_empty() {
        return None;
    }
    if !whole.is_empty() && !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let whole = if whole.is_empty() {
        0
    } else {
        whole.parse::<u32>().ok()?
    };
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u32>()
            .ok()?
            .saturating_mul(match fraction.len() {
                1 => 100,
                2 => 10,
                _ => 1,
            })
    };
    whole.checked_mul(1_000)?.checked_add(fraction)
}

fn parse_white_space(value: &str) -> Option<WhiteSpaceValue> {
    match value.to_ascii_lowercase().as_str() {
        "normal" => Some(WhiteSpaceValue::Normal),
        "pre-line" => Some(WhiteSpaceValue::PreLine),
        "pre" => Some(WhiteSpaceValue::Pre),
        "pre-wrap" => Some(WhiteSpaceValue::PreWrap),
        "nowrap" => Some(WhiteSpaceValue::NoWrap),
        _ => None,
    }
}

fn parse_text_align(value: &str) -> Option<TextAlignValue> {
    match value.to_ascii_lowercase().as_str() {
        "left" => Some(TextAlignValue::Left),
        "center" => Some(TextAlignValue::Center),
        "right" => Some(TextAlignValue::Right),
        _ => None,
    }
}

fn parse_justify_content(value: &str) -> Option<JustifyContentValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(JustifyContentValue::FlexStart),
        "center" => Some(JustifyContentValue::Center),
        "flex-end" => Some(JustifyContentValue::FlexEnd),
        "space-between" => Some(JustifyContentValue::SpaceBetween),
        _ => None,
    }
}

fn parse_text_decoration(value: &str) -> Option<TextDecorationValue> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(TextDecorationValue::None),
        "underline" => Some(TextDecorationValue::Underline),
        _ => None,
    }
}

fn parse_text_transform(value: &str) -> Option<TextTransformValue> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(TextTransformValue::None),
        "uppercase" => Some(TextTransformValue::Uppercase),
        "lowercase" => Some(TextTransformValue::Lowercase),
        _ => None,
    }
}

fn parse_font_weight(value: &str) -> Option<FontWeightValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" | "400" => Some(FontWeightValue::Normal),
        "bold" | "700" => Some(FontWeightValue::Bold),
        _ => None,
    }
}

fn parse_font_style(value: &str) -> Option<FontStyleValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(FontStyleValue::Normal),
        "italic" => Some(FontStyleValue::Italic),
        _ => None,
    }
}

fn parse_word_break(value: &str) -> Option<WordBreakValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(WordBreakValue::Normal),
        "break-all" => Some(WordBreakValue::BreakAll),
        _ => None,
    }
}

fn parse_text_overflow(value: &str) -> Option<TextOverflowValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "clip" => Some(TextOverflowValue::Clip),
        "ellipsis" => Some(TextOverflowValue::Ellipsis),
        _ => None,
    }
}

fn parse_vertical_align(value: &str) -> Option<VerticalAlignValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "baseline" => Some(VerticalAlignValue::Baseline),
        "top" => Some(VerticalAlignValue::Top),
        "middle" => Some(VerticalAlignValue::Middle),
        "bottom" => Some(VerticalAlignValue::Bottom),
        _ => None,
    }
}

fn parse_visibility(value: &str) -> Option<VisibilityValue> {
    match value.to_ascii_lowercase().as_str() {
        "hidden" => Some(VisibilityValue::Hidden),
        "visible" => Some(VisibilityValue::Other),
        _ => None,
    }
}

fn parse_overflow(value: &str) -> Option<OverflowValue> {
    match value.to_ascii_lowercase().as_str() {
        "hidden" => Some(OverflowValue::Hidden),
        "clip" => Some(OverflowValue::Clip),
        "visible" | "auto" | "scroll" => Some(OverflowValue::Other),
        _ => None,
    }
}

fn parse_selector(source: &str) -> Option<NativeSelector> {
    let source = source.trim();
    if source.is_empty() || source.len() > MAX_SELECTOR_BYTES {
        return None;
    }
    let compound_sources = split_selector_compounds(source)?;
    if compound_sources.len() > MAX_SELECTOR_PARTS {
        return None;
    }
    let compounds = compound_sources
        .into_iter()
        .map(parse_compound_selector)
        .collect::<Option<Vec<_>>>()?;
    let specificity = compounds.iter().fold(0u16, |specificity, compound| {
        specificity.saturating_add(compound.specificity)
    });
    Some(NativeSelector {
        compounds,
        specificity,
    })
}

fn split_selector_compounds(source: &str) -> Option<Vec<&str>> {
    let bytes = source.as_bytes();
    let mut compounds = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let mut bracket_depth = 0usize;
    let mut quote = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' if bracket_depth > 0 => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
            }
            byte if byte.is_ascii_whitespace() && bracket_depth == 0 => {
                if start < cursor {
                    compounds.push(&source[start..cursor]);
                }
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                start = cursor;
                continue;
            }
            _ => {}
        }
        cursor += 1;
    }
    if bracket_depth != 0 || quote.is_some() {
        return None;
    }
    if start < source.len() {
        compounds.push(&source[start..]);
    }
    (!compounds.is_empty()).then_some(compounds)
}

fn parse_compound_selector(source: &str) -> Option<NativeCompoundSelector> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut selector = NativeCompoundSelector {
        tag: None,
        id: None,
        classes: Vec::new(),
        attributes: Vec::new(),
        specificity: 0,
    };
    if bytes.first() == Some(&b'*') {
        cursor += 1;
    } else if bytes.first().is_some_and(|byte| is_identifier_start(*byte)) {
        let (name, next) = read_identifier(source, cursor)?;
        selector.tag = Some(name.to_ascii_lowercase());
        selector.specificity = 1;
        cursor = next;
    }
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'#' => {
                if selector.id.is_some() {
                    return None;
                }
                let (id, next) = read_identifier(source, cursor + 1)?;
                selector.id = Some(id);
                selector.specificity = selector.specificity.saturating_add(100);
                cursor = next;
            }
            b'.' => {
                let (class, next) = read_identifier(source, cursor + 1)?;
                selector.classes.push(class);
                selector.specificity = selector.specificity.saturating_add(10);
                cursor = next;
            }
            b'[' => {
                let close = source[cursor + 1..].find(']')? + cursor + 1;
                let content = source[cursor + 1..close].trim();
                let (name, value) = parse_attribute_selector(content)?;
                selector
                    .attributes
                    .push(NativeAttributeSelector { name, value });
                selector.specificity = selector.specificity.saturating_add(10);
                cursor = close + 1;
            }
            _ => return None,
        }
    }
    (selector.tag.is_some()
        || selector.id.is_some()
        || !selector.classes.is_empty()
        || !selector.attributes.is_empty()
        || source == "*")
        .then_some(selector)
}

fn parse_attribute_selector(source: &str) -> Option<(String, Option<String>)> {
    let (name, raw_value) = source
        .split_once('=')
        .map_or((source, None), |(name, value)| (name, Some(value.trim())));
    let name = name.trim();
    if name.is_empty()
        || !name.bytes().all(is_identifier_char)
        || !is_identifier_start(name.as_bytes()[0])
    {
        return None;
    }
    let value = match raw_value {
        Some(value)
            if value.len() >= 2
                && matches!(
                    (value.as_bytes().first(), value.as_bytes().last()),
                    (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\''))
                ) =>
        {
            Some(value[1..value.len() - 1].to_owned())
        }
        Some(value)
            if value.is_empty()
                || value.chars().any(char::is_whitespace)
                || value.contains(['"', '\'']) =>
        {
            return None;
        }
        Some(value) => Some(value.to_owned()),
        None => None,
    };
    Some((name.to_ascii_lowercase(), value))
}

fn read_identifier(source: &str, start: usize) -> Option<(String, usize)> {
    if start >= source.len() || !is_identifier_start(source.as_bytes()[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < source.len() && is_identifier_char(source.as_bytes()[end]) {
        end += 1;
    }
    Some((source[start..end].to_owned(), end))
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-')
}

fn is_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn strip_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find("/*") {
        let start = cursor + relative_start;
        output.push_str(&source[cursor..start]);
        let Some(relative_end) = source[start + 2..].find("*/") else {
            return output;
        };
        cursor = start + 2 + relative_end + 2;
    }
    output.push_str(&source[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::native_engine::config::NativeEngineLimits;

    fn node(html: &str) -> NativeNode {
        let document =
            super::super::dom::NativeDocument::parse(html, &NativeEngineLimits::default())
                .expect("fixture document");
        let root = document.node(document.root()).expect("document root");
        let child = *root.children().first().expect("fixture element");
        document.node(child).expect("fixture element").clone()
    }

    fn border_side(width: u32, color: NativeColor) -> NativeBorderSide {
        styled_border_side(width, NativeBorderStyle::Solid, color)
    }

    fn styled_border_side(
        width: u32,
        style: NativeBorderStyle,
        color: NativeColor,
    ) -> NativeBorderSide {
        NativeBorderSide {
            width,
            style,
            color,
        }
    }

    fn uniform_border(width: u32, color: NativeColor) -> NativeBorder {
        let side = border_side(width, color);
        NativeBorder {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    #[test]
    fn selector_parser_supports_bounded_compound_and_descendant_selectors() {
        let selector = parse_selector("main .card button[data-state=ready]").unwrap();
        assert_eq!(selector.specificity, 22);
        assert!(parse_selector("button[data-label='ready now']").is_some());
        assert!(parse_selector("main > button").is_none());
        assert!(parse_selector("main + button").is_none());
        assert!(parse_selector("button:hover").is_none());
        let too_many = ["div"; MAX_SELECTOR_PARTS + 1].join(" ");
        assert!(parse_selector(&too_many).is_none());
        assert_eq!(
            selector_diagnostic_detail(&too_many),
            "selector-too-complex"
        );
    }

    #[test]
    fn descendant_selector_matches_owned_ancestors_before_cascade() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "main .card button { display: none; background-color: red; } main button { display: block; }"
                .into(),
        ])
        .unwrap();
        let document = NativeDocument::parse(
            "<main><section class='card'><button id='target'>Target</button></section><button id='other'>Other</button></main>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let target = document.resolve_target("id=target").unwrap();
        let other = document.resolve_target("id=other").unwrap();

        let target_style = stylesheet.computed_for_in_document(&document, target, None);
        assert!(target_style.hidden());
        assert_eq!(target_style.background_color(), Some(NativeColor::RED));

        let other_style = stylesheet.computed_for_in_document(&document, other, None);
        assert!(!other_style.hidden());
        assert_eq!(other_style.background_color(), None);
    }

    #[test]
    fn declarations_parse_only_supported_presentation_properties() {
        let declarations = parse_declarations(
            "color: red; display: none !important; visibility: visible; opacity: 50%; white-space: pre-line; text-align: center; justify-content: space-between; text-decoration: underline; text-indent: 12px; word-spacing: 12px; letter-spacing: 12px; gap: 12px; font-weight: bold; font-style: italic; word-break: break-all; text-overflow: ellipsis; vertical-align: bottom; width: 240px; height: 30px; min-width: 12px; max-width: 400px; min-height: 14px; max-height: 500px; line-height: 28px; border: 2px solid #102030; border-radius: 1px 2px 3px 4px; padding: 4px; margin: 3px; box-sizing: border-box; overflow: hidden",
        );
        assert_eq!(declarations.display, Some(DisplayValue::None));
        assert_eq!(declarations.visibility, Some(VisibilityValue::Other));
        assert_eq!(declarations.opacity, Some(128));
        assert_eq!(declarations.white_space, Some(WhiteSpaceValue::PreLine));
        assert_eq!(declarations.text_align, Some(TextAlignValue::Center));
        assert_eq!(
            declarations.justify_content,
            Some(JustifyContentValue::SpaceBetween)
        );
        assert_eq!(
            declarations.text_decoration,
            Some(TextDecorationValue::Underline)
        );
        assert_eq!(declarations.text_indent, Some(12));
        assert_eq!(declarations.word_spacing, Some(12));
        assert_eq!(declarations.letter_spacing, Some(12));
        assert_eq!(declarations.gap, Some(12));
        assert_eq!(declarations.font_weight, Some(FontWeightValue::Bold));
        assert_eq!(declarations.font_style, Some(FontStyleValue::Italic));
        assert_eq!(declarations.word_break, Some(WordBreakValue::BreakAll));
        assert_eq!(
            declarations.text_overflow,
            Some(TextOverflowValue::Ellipsis)
        );
        assert_eq!(
            declarations.vertical_align,
            Some(VerticalAlignValue::Bottom)
        );
        assert_eq!(declarations.width, Some(240));
        assert_eq!(declarations.height, Some(30));
        assert_eq!(declarations.min_width, Some(12));
        assert_eq!(declarations.max_width, Some(400));
        assert_eq!(declarations.min_height, Some(14));
        assert_eq!(declarations.max_height, Some(500));
        assert_eq!(declarations.line_height, Some(28));
        assert_eq!(declarations.color, Some(NativeColor::RED));
        let parsed_color = NativeColor {
            red: 16,
            green: 32,
            blue: 48,
            alpha: 255,
        };
        assert_eq!(declarations.border, [Some(border_side(2, parsed_color)); 4]);
        assert_eq!(
            declarations.border_radius,
            Some(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 4,
            })
        );
        assert_eq!(declarations.padding, [Some(4); 4]);
        assert_eq!(declarations.margin, [Some(3); 4]);
        assert_eq!(declarations.box_sizing, Some(NativeBoxSizing::BorderBox));
        assert_eq!(declarations.overflow, Some(OverflowValue::Hidden));
        assert_eq!(declarations.overflow_x, Some(OverflowValue::Hidden));
        assert_eq!(declarations.overflow_y, Some(OverflowValue::Hidden));
        assert_eq!(parse_white_space("normal"), Some(WhiteSpaceValue::Normal));
        assert_eq!(
            parse_white_space("pre-line"),
            Some(WhiteSpaceValue::PreLine)
        );
        assert_eq!(parse_white_space("pre"), Some(WhiteSpaceValue::Pre));
        assert_eq!(
            parse_white_space("pre-wrap"),
            Some(WhiteSpaceValue::PreWrap)
        );
        assert_eq!(parse_white_space("nowrap"), Some(WhiteSpaceValue::NoWrap));
        assert_eq!(parse_display("flex"), Some(DisplayValue::Flex));
        assert_eq!(parse_display("FLEX"), Some(DisplayValue::Flex));
        assert_eq!(parse_display("grid"), Some(DisplayValue::Other));
        assert_eq!(parse_overflow("scroll"), Some(OverflowValue::Other));
        assert_eq!(parse_overflow("visible"), Some(OverflowValue::Other));
        assert_eq!(parse_overflow("auto"), Some(OverflowValue::Other));
        assert_eq!(parse_overflow("clip"), Some(OverflowValue::Clip));
        assert_eq!(
            parse_border("1px dashed red"),
            Some(styled_border_side(
                1,
                NativeBorderStyle::Dashed,
                NativeColor::RED
            ))
        );
        assert_eq!(
            parse_border("2px DOTTED blue"),
            Some(styled_border_side(
                2,
                NativeBorderStyle::Dotted,
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                }
            ))
        );
        let unsupported_sides = parse_declarations(
            "border-top: 1px double red; border-right: -1px solid blue; border-bottom: 20000px solid red; border-left: 1em solid green",
        );
        assert_eq!(unsupported_sides.border, [None; 4]);
        assert_eq!(
            parse_border("1px solid rgb(1, 2, 3)"),
            Some(border_side(
                1,
                NativeColor {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 255,
                },
            ))
        );
    }

    #[test]
    fn functional_alpha_color_parser_accepts_bounded_rgba_values() {
        assert_eq!(
            parse_color("RGBA( 16, 32, 48, 0.5 )"),
            Some(NativeColor {
                red: 16,
                green: 32,
                blue: 48,
                alpha: 128,
            })
        );
        assert_eq!(
            parse_color("rgba(255,0,1,50%)"),
            Some(NativeColor {
                red: 255,
                green: 0,
                blue: 1,
                alpha: 128,
            })
        );
        assert_eq!(
            parse_color("rgba(0, 0, 0, 0)"),
            Some(NativeColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            })
        );
        assert_eq!(parse_color("rgba(256, 0, 0, 1)"), None);
        assert_eq!(parse_color("rgba(0, -1, 0, 1)"), None);
        assert_eq!(parse_color("rgba(0, 0, 0, 1.001)"), None);
        assert_eq!(parse_color("rgba(0, 0, 0, 101%)"), None);
        assert_eq!(parse_color("rgba(0, 0, 0, 1, 0)"), None);
        assert_eq!(parse_color("rgba(0%, 0, 0, 1)"), None);
        assert_eq!(parse_color("rgb(0 0 0 / 0.5)"), None);
    }

    #[test]
    fn border_radius_parser_expands_bounded_physical_shorthand() {
        assert_eq!(
            parse_border_radius("5px"),
            Some(NativeBorderRadius {
                top_left: 5,
                top_right: 5,
                bottom_right: 5,
                bottom_left: 5,
            })
        );
        assert_eq!(
            parse_border_radius("1px 2px"),
            Some(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 1,
                bottom_left: 2,
            })
        );
        assert_eq!(
            parse_border_radius("1px 2px 3px"),
            Some(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 2,
            })
        );
        assert_eq!(parse_border_radius("1px/2px"), None);
        assert_eq!(parse_border_radius("50%"), None);
        assert_eq!(parse_border_radius("-1px"), None);
        assert_eq!(parse_border_radius("1px 2px 3px 4px 5px"), None);
        assert_eq!(parse_border_radius("20000px"), None);
    }

    #[test]
    fn box_edges_parser_expands_bounded_physical_values() {
        assert_eq!(parse_box_edges("4px"), Some([4; 4]));
        assert_eq!(parse_box_edges("1px 2px"), Some([1, 2, 1, 2]));
        assert_eq!(parse_box_edges("1px 2px 3px"), Some([1, 2, 3, 2]));
        assert_eq!(parse_box_edges("1px 2px 3px 4px"), Some([1, 2, 3, 4]));
        assert_eq!(parse_box_edges("1px 2px 3px 4px 5px"), None);
        assert_eq!(parse_box_edges("50%"), None);
        assert_eq!(parse_box_edges("-1px"), None);

        let declarations = parse_declarations(
            "padding: 1px 2px 3px 4px; padding-left: 5px; margin: 6px 7px; margin-bottom: 8px",
        );
        assert_eq!(declarations.padding, [Some(1), Some(2), Some(3), Some(5)]);
        assert_eq!(declarations.margin, [Some(6), Some(7), Some(8), Some(7)]);
        let declarations = parse_declarations("margin: 6px 7px; margin-bottom: 8px");
        assert_eq!(declarations.margin, [Some(6), Some(7), Some(8), Some(7)]);
        let declarations =
            parse_declarations("padding: 4px; padding-left: 50%; margin: 2px; margin-top: -1px");
        assert_eq!(declarations.padding, [Some(4); 4]);
        assert_eq!(declarations.margin, [Some(2); 4]);
    }

    #[test]
    fn stylesheet_cascade_prefers_specificity_then_inline_style() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "button { display: none; } #shown { display: block; }".into(),
        ])
        .unwrap();
        let node = node("<button id='shown' style='display:none'>Shown</button>");
        assert!(stylesheet.computed_for(&node).hidden());
    }

    #[test]
    fn stylesheet_cascade_resolves_uniform_border_with_inline_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { border: 1px solid red; } #card { border: 2px solid blue; }".into(),
        ])
        .unwrap();
        let node = node("<div id='card' style='border: 3px solid green'>Card</div>");
        assert_eq!(
            stylesheet.computed_for(&node).border(),
            Some(uniform_border(
                3,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                },
            ))
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_border_radius_with_inline_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            ".card { border-radius: 1px; } #card { border-radius: 2px 3px; }".into(),
        ])
        .unwrap();
        let node =
            node("<div id='card' class='card' style='border-radius: 4px 5px 6px 7px'>Card</div>");
        assert_eq!(
            stylesheet.computed_for(&node).border_radius(),
            NativeBorderRadius {
                top_left: 4,
                top_right: 5,
                bottom_right: 6,
                bottom_left: 7,
            }
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_box_edges_per_physical_side() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { padding: 1px 2px; margin: 3px; } #card { padding-left: 5px; margin-top: 4px; }"
                .into(),
        ])
        .unwrap();
        let node = node("<div id='card' style='padding-bottom:6px;margin-right:7px'>Card</div>");
        let style = stylesheet.computed_for(&node);
        assert_eq!(
            style.padding(),
            NativeBoxEdges {
                top: 1,
                right: 2,
                bottom: 6,
                left: 5,
            }
        );
        assert_eq!(
            style.margin(),
            NativeBoxEdges {
                top: 4,
                right: 7,
                bottom: 3,
                left: 3,
            }
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_line_height_with_inline_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            ".card { line-height: 16px; } #card { line-height: 20px; }".into(),
        ])
        .unwrap();
        let node = node("<div id='card' class='card' style='line-height: 28px'>Card</div>");
        assert_eq!(stylesheet.computed_for(&node).line_height(), Some(28));
    }

    #[test]
    fn overflow_axis_longhands_cascade_independently_from_shorthand() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            ".card { overflow: hidden; } #card { overflow-x: clip; overflow-y: visible; }".into(),
        ])
        .unwrap();
        let node = node("<div id='card' class='card'>Card</div>");
        let style = stylesheet.computed_for(&node);

        assert!(style.overflow_clip_x());
        assert!(!style.overflow_clip_y());

        let declarations =
            parse_declarations("overflow-x: hidden; overflow: clip; overflow-y: visible;");
        assert_eq!(declarations.overflow_x, Some(OverflowValue::Clip));
        assert_eq!(declarations.overflow_y, Some(OverflowValue::Other));
    }

    #[test]
    fn stylesheet_cascade_resolves_min_max_dimensions_with_inline_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { min-width: 8px; max-width: 64px; min-height: 10px; max-height: 80px; } #card { min-width: 16px; max-height: 40px; }"
                .into(),
        ])
        .unwrap();
        let node = node("<div id='card' style='max-width: 32px; min-height: 20px'>Card</div>");
        let style = stylesheet.computed_for(&node);

        assert_eq!(style.min_width(), Some(16));
        assert_eq!(style.max_width(), Some(32));
        assert_eq!(style.min_height(), Some(20));
        assert_eq!(style.max_height(), Some(40));
    }

    #[test]
    fn computed_style_inherits_fixed_line_height_and_preserves_child_precedence() {
        let document = NativeDocument::parse(
            "<style>#parent { line-height: 28px; } #explicit { line-height: 32px; }</style><div id='parent'><section id='child'>Child</section><section id='explicit'>Explicit</section><section id='invalid' style='line-height:0px'>Invalid</section></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).line_height(),
            Some(28)
        );
        assert_eq!(
            document.computed_style_for_layout(child).line_height(),
            Some(28)
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).line_height(),
            Some(32)
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).line_height(),
            Some(28)
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_physical_border_sides_independently() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { border: 1px solid red; } #card { border-left: 3px solid blue; }".into(),
        ])
        .unwrap();
        let node = node("<div id='card' style='border-top: 2px solid green'>Card</div>");
        assert_eq!(
            stylesheet.computed_for(&node).border(),
            Some(NativeBorder {
                top: border_side(
                    2,
                    NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 255,
                    },
                ),
                right: border_side(1, NativeColor::RED),
                bottom: border_side(1, NativeColor::RED),
                left: border_side(
                    3,
                    NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    },
                ),
            })
        );
    }

    #[test]
    fn dimension_parser_rejects_non_pixel_or_unbounded_values() {
        assert_eq!(parse_dimension("240px"), Some(240));
        assert_eq!(parse_dimension("240"), None);
        assert_eq!(parse_dimension("50%"), None);
        assert_eq!(parse_dimension("20000px"), None);
    }

    #[test]
    fn line_height_parser_accepts_only_positive_bounded_pixels() {
        assert_eq!(parse_line_height("28px"), Some(28));
        assert_eq!(parse_line_height("0px"), None);
        assert_eq!(parse_line_height("normal"), None);
        assert_eq!(parse_line_height("1.5"), None);
        assert_eq!(parse_line_height("50%"), None);
        assert_eq!(parse_line_height("20000px"), None);
    }

    #[test]
    fn text_indent_parser_accepts_only_bounded_non_negative_pixels() {
        assert_eq!(parse_dimension("16px"), Some(16));
        assert_eq!(parse_dimension("0px"), Some(0));
        assert_eq!(parse_dimension("-1px"), None);
        assert_eq!(parse_dimension("1.5px"), None);
        assert_eq!(parse_dimension("2em"), None);
        assert_eq!(parse_dimension("50%"), None);
        assert_eq!(parse_dimension("20000px"), None);
    }

    #[test]
    fn word_spacing_parser_accepts_only_bounded_non_negative_pixels() {
        assert_eq!(parse_dimension("16px"), Some(16));
        assert_eq!(parse_dimension("0px"), Some(0));
        assert_eq!(parse_dimension("-1px"), None);
        assert_eq!(parse_dimension("1.5px"), None);
        assert_eq!(parse_dimension("2em"), None);
        assert_eq!(parse_dimension("50%"), None);
        assert_eq!(parse_dimension("20000px"), None);
    }

    #[test]
    fn letter_spacing_parser_accepts_only_bounded_non_negative_pixels() {
        assert_eq!(parse_dimension("16px"), Some(16));
        assert_eq!(parse_dimension("0px"), Some(0));
        assert_eq!(parse_dimension("-1px"), None);
        assert_eq!(parse_dimension("1.5px"), None);
        assert_eq!(parse_dimension("2em"), None);
        assert_eq!(parse_dimension("50%"), None);
        assert_eq!(parse_dimension("normal"), None);
        assert_eq!(parse_dimension("20000px"), None);
    }

    #[test]
    fn gap_parser_accepts_only_bounded_non_negative_single_pixels() {
        assert_eq!(parse_declarations("gap: 16px").gap, Some(16));
        assert_eq!(parse_declarations("gap: 0px").gap, Some(0));
        assert_eq!(parse_declarations("gap: -1px").gap, None);
        assert_eq!(parse_declarations("gap: 1px 2px").gap, None);
        assert_eq!(parse_declarations("gap: 1.5px").gap, None);
        assert_eq!(parse_declarations("gap: 2em").gap, None);
        assert_eq!(parse_declarations("gap: 50%").gap, None);
        assert_eq!(parse_declarations("gap: 20000px").gap, None);
    }

    #[test]
    fn opacity_parser_quantizes_bounded_numbers_and_percentages() {
        assert_eq!(parse_opacity("0"), Some(0));
        assert_eq!(parse_opacity("0.5"), Some(128));
        assert_eq!(parse_opacity(".5"), Some(128));
        assert_eq!(parse_opacity("1"), Some(255));
        assert_eq!(parse_opacity("50%"), Some(128));
        assert_eq!(parse_opacity("100%"), Some(255));
        assert_eq!(parse_opacity("1.001"), None);
        assert_eq!(parse_opacity("101%"), None);
        assert_eq!(parse_opacity("-0.1"), None);
        assert_eq!(parse_opacity("0.1234"), None);
        assert_eq!(parse_opacity("1e-1"), None);
    }

    #[test]
    fn text_align_parser_accepts_only_bounded_physical_values() {
        assert_eq!(parse_text_align("left"), Some(TextAlignValue::Left));
        assert_eq!(parse_text_align("CENTER"), Some(TextAlignValue::Center));
        assert_eq!(parse_text_align("right"), Some(TextAlignValue::Right));
        assert_eq!(parse_text_align("justify"), None);
        assert_eq!(parse_text_align("start"), None);
        assert_eq!(parse_text_align("end"), None);
    }

    #[test]
    fn justify_content_parser_accepts_only_bounded_row_values() {
        assert_eq!(
            parse_justify_content("flex-start"),
            Some(JustifyContentValue::FlexStart)
        );
        assert_eq!(
            parse_justify_content("CENTER"),
            Some(JustifyContentValue::Center)
        );
        assert_eq!(
            parse_justify_content("flex-end"),
            Some(JustifyContentValue::FlexEnd)
        );
        assert_eq!(
            parse_justify_content("space-between"),
            Some(JustifyContentValue::SpaceBetween)
        );
        assert_eq!(parse_justify_content("normal"), None);
        assert_eq!(parse_justify_content("space-around"), None);
        assert_eq!(parse_justify_content("space-evenly"), None);
        assert_eq!(parse_justify_content("start"), None);
    }

    #[test]
    fn text_decoration_parser_accepts_only_bounded_lines() {
        assert_eq!(
            parse_text_decoration("UNDERLINE"),
            Some(TextDecorationValue::Underline)
        );
        assert_eq!(
            parse_text_decoration("none"),
            Some(TextDecorationValue::None)
        );
        assert_eq!(parse_text_decoration("overline"), None);
        assert_eq!(parse_text_decoration("underline line-through"), None);
        assert_eq!(parse_text_decoration("underline red"), None);
    }

    #[test]
    fn text_transform_parser_accepts_only_bounded_ascii_modes() {
        assert_eq!(
            parse_text_transform("UPPERCASE"),
            Some(TextTransformValue::Uppercase)
        );
        assert_eq!(
            parse_text_transform("lowercase"),
            Some(TextTransformValue::Lowercase)
        );
        assert_eq!(parse_text_transform("none"), Some(TextTransformValue::None));
        assert_eq!(parse_text_transform("capitalize"), None);
        assert_eq!(parse_text_transform("full-width"), None);
        assert_eq!(parse_text_transform("initial"), None);
    }

    #[test]
    fn font_weight_parser_normalizes_only_bounded_normal_and_bold_pairs() {
        assert_eq!(parse_font_weight("normal"), Some(FontWeightValue::Normal));
        assert_eq!(parse_font_weight("400"), Some(FontWeightValue::Normal));
        assert_eq!(parse_font_weight("BOLD"), Some(FontWeightValue::Bold));
        assert_eq!(parse_font_weight("700"), Some(FontWeightValue::Bold));
        assert_eq!(parse_font_weight("500"), None);
        assert_eq!(parse_font_weight("lighter"), None);
        assert_eq!(parse_font_weight("700 800"), None);
        assert_eq!(parse_font_weight("initial"), None);
    }

    #[test]
    fn font_style_parser_accepts_only_normal_and_italic() {
        assert_eq!(parse_font_style("normal"), Some(FontStyleValue::Normal));
        assert_eq!(parse_font_style("ITALIC"), Some(FontStyleValue::Italic));
        assert_eq!(parse_font_style("oblique"), None);
        assert_eq!(parse_font_style("12deg"), None);
        assert_eq!(parse_font_style("initial"), None);
    }

    #[test]
    fn word_break_parser_accepts_only_normal_and_break_all() {
        assert_eq!(parse_word_break("normal"), Some(WordBreakValue::Normal));
        assert_eq!(
            parse_word_break("BREAK-ALL"),
            Some(WordBreakValue::BreakAll)
        );
        assert_eq!(parse_word_break("keep-all"), None);
        assert_eq!(parse_word_break("break-word"), None);
        assert_eq!(parse_word_break("initial"), None);
    }

    #[test]
    fn text_overflow_parser_accepts_only_clip_and_ellipsis() {
        assert_eq!(parse_text_overflow("clip"), Some(TextOverflowValue::Clip));
        assert_eq!(
            parse_text_overflow("ELLIPSIS"),
            Some(TextOverflowValue::Ellipsis)
        );
        assert_eq!(parse_text_overflow("fade"), None);
        assert_eq!(parse_text_overflow("initial"), None);
        assert_eq!(parse_text_overflow(""), None);
    }

    #[test]
    fn vertical_align_parser_accepts_only_bounded_keywords() {
        assert_eq!(
            parse_vertical_align("BASELINE"),
            Some(VerticalAlignValue::Baseline)
        );
        assert_eq!(parse_vertical_align("top"), Some(VerticalAlignValue::Top));
        assert_eq!(
            parse_vertical_align("middle"),
            Some(VerticalAlignValue::Middle)
        );
        assert_eq!(
            parse_vertical_align("BOTTOM"),
            Some(VerticalAlignValue::Bottom)
        );
        assert_eq!(parse_vertical_align("text-top"), None);
        assert_eq!(parse_vertical_align("sub"), None);
        assert_eq!(parse_vertical_align("1px"), None);
        assert_eq!(parse_vertical_align("initial"), None);
    }

    #[test]
    fn opacity_is_cascaded_locally_without_inheriting_to_children() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { opacity: 25%; } #target { opacity: 75%; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(stylesheet.computed_for(&node).opacity(), 191);

        let document = NativeDocument::parse(
            "<style>#parent { opacity: 50%; }</style><div id='parent'><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        assert_eq!(document.computed_style_for_layout(parent).opacity(), 128);
        assert_eq!(document.computed_style_for_layout(child).opacity(), 255);
    }

    #[test]
    fn text_align_is_cascaded_and_inherited_with_child_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-align: left; } #target { text-align: right; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target' style='text-align: center'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_align(),
            TextAlignValue::Center
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-align: center; } #explicit { text-align: right; } #invalid { text-align: justify; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_align(),
            TextAlignValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_align(),
            TextAlignValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).text_align(),
            TextAlignValue::Right
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).text_align(),
            TextAlignValue::Center
        );
    }

    #[test]
    fn text_decoration_is_inherited_and_child_none_clears_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-decoration: none; } #target { text-decoration: underline; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_decoration(),
            TextDecorationValue::Underline
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-decoration: underline; } #clear { text-decoration: none; } #invalid { text-decoration: overline; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_decoration(),
            TextDecorationValue::Underline
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_decoration(),
            TextDecorationValue::Underline
        );
        assert_eq!(
            document.computed_style_for_layout(clear).text_decoration(),
            TextDecorationValue::None
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration(),
            TextDecorationValue::Underline
        );
    }

    #[test]
    fn text_transform_is_inherited_and_child_none_clears_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-transform: uppercase; } #target { text-transform: lowercase; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_transform(),
            TextTransformValue::Lowercase
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-transform: uppercase; } #clear { text-transform: none; } #invalid { text-transform: capitalize; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_transform(),
            TextTransformValue::Uppercase
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_transform(),
            TextTransformValue::Uppercase
        );
        assert_eq!(
            document.computed_style_for_layout(clear).text_transform(),
            TextTransformValue::None
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).text_transform(),
            TextTransformValue::Uppercase
        );
    }

    #[test]
    fn font_weight_is_inherited_and_child_normal_clears_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { font-weight: bold; } #target { font-weight: 400; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).font_weight(),
            FontWeightValue::Normal
        );

        let document = NativeDocument::parse(
            "<style>#parent { font-weight: 700; } #clear { font-weight: normal; } #invalid { font-weight: 500; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).font_weight(),
            FontWeightValue::Bold
        );
        assert_eq!(
            document.computed_style_for_layout(child).font_weight(),
            FontWeightValue::Bold
        );
        assert_eq!(
            document.computed_style_for_layout(clear).font_weight(),
            FontWeightValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).font_weight(),
            FontWeightValue::Bold
        );
    }

    #[test]
    fn font_style_is_inherited_and_child_normal_clears_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { font-style: italic; } #target { font-style: normal; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).font_style(),
            FontStyleValue::Normal
        );

        let document = NativeDocument::parse(
            "<style>#parent { font-style: italic; } #clear { font-style: normal; } #invalid { font-style: oblique; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).font_style(),
            FontStyleValue::Italic
        );
        assert_eq!(
            document.computed_style_for_layout(child).font_style(),
            FontStyleValue::Italic
        );
        assert_eq!(
            document.computed_style_for_layout(clear).font_style(),
            FontStyleValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).font_style(),
            FontStyleValue::Italic
        );
    }

    #[test]
    fn word_break_is_inherited_and_child_normal_clears_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { word-break: break-all; } #target { word-break: normal; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).word_break(),
            WordBreakValue::Normal
        );

        let document = NativeDocument::parse(
            "<style>#parent { word-break: break-all; } #clear { word-break: normal; } #invalid { word-break: keep-all; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).word_break(),
            WordBreakValue::BreakAll
        );
        assert_eq!(
            document.computed_style_for_layout(child).word_break(),
            WordBreakValue::BreakAll
        );
        assert_eq!(
            document.computed_style_for_layout(clear).word_break(),
            WordBreakValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).word_break(),
            WordBreakValue::BreakAll
        );
    }

    #[test]
    fn text_overflow_is_local_and_defaults_to_clip() {
        let document = NativeDocument::parse(
            "<style>#parent { text-overflow: ellipsis; } #child { text-overflow: clip; }</style><div id='parent'><span id='child'>Child</span><span id='other'>Other</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let other = document.resolve_target("id=other").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_overflow(),
            TextOverflowValue::Ellipsis
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_overflow(),
            TextOverflowValue::Clip
        );
        assert_eq!(
            document.computed_style_for_layout(other).text_overflow(),
            TextOverflowValue::Clip
        );

        let defaults = NativeDocument::parse(
            "<div id='default'>Default</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let default_node = defaults.resolve_target("id=default").unwrap();
        assert_eq!(
            defaults
                .computed_style_for_layout(default_node)
                .text_overflow(),
            TextOverflowValue::Clip
        );
    }

    #[test]
    fn vertical_align_is_inherited_and_child_override_replaces_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { vertical-align: top; } #target { vertical-align: bottom; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).vertical_align(),
            VerticalAlignValue::Bottom
        );

        let document = NativeDocument::parse(
            "<style>#parent { vertical-align: middle; } #explicit { vertical-align: bottom; } #clear { vertical-align: baseline; } #invalid { vertical-align: sub; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).vertical_align(),
            VerticalAlignValue::Middle
        );
        assert_eq!(
            document.computed_style_for_layout(child).vertical_align(),
            VerticalAlignValue::Middle
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .vertical_align(),
            VerticalAlignValue::Bottom
        );
        assert_eq!(
            document.computed_style_for_layout(clear).vertical_align(),
            VerticalAlignValue::Baseline
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).vertical_align(),
            VerticalAlignValue::Middle
        );
    }

    #[test]
    fn text_indent_is_local_and_does_not_inherit() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-indent: 16px; } #target { text-indent: 24px; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(stylesheet.computed_for(&node).text_indent(), 24);

        let document = NativeDocument::parse(
            "<style>#parent { text-indent: 16px; } #explicit { text-indent: 24px; } #invalid { text-indent: -1px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(document.computed_style_for_layout(parent).text_indent(), 16);
        assert_eq!(document.computed_style_for_layout(child).text_indent(), 0);
        assert_eq!(
            document.computed_style_for_layout(explicit).text_indent(),
            24
        );
        assert_eq!(document.computed_style_for_layout(invalid).text_indent(), 0);
    }

    #[test]
    fn word_spacing_is_inherited_and_child_override_replaces_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { word-spacing: 16px; letter-spacing: 16px; } #target { word-spacing: 24px; letter-spacing: 24px; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(stylesheet.computed_for(&node).word_spacing(), 24);
        assert_eq!(stylesheet.computed_for(&node).letter_spacing(), 24);

        let document = NativeDocument::parse(
            "<style>#parent { word-spacing: 16px; letter-spacing: 16px; } #explicit { word-spacing: 24px; letter-spacing: 24px; } #invalid { word-spacing: -1px; letter-spacing: -1px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).word_spacing(),
            16
        );
        assert_eq!(document.computed_style_for_layout(child).word_spacing(), 16);
        assert_eq!(
            document.computed_style_for_layout(child).letter_spacing(),
            16
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).word_spacing(),
            24
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .letter_spacing(),
            24
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).word_spacing(),
            16
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).letter_spacing(),
            16
        );
    }

    #[test]
    fn gap_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { gap: 16px; } #explicit { gap: 24px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='gap: 32px'>Explicit</span><span id='invalid' style='gap: -1px'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(document.computed_style_for_layout(parent).gap(), 16);
        assert_eq!(document.computed_style_for_layout(child).gap(), 0);
        assert_eq!(document.computed_style_for_layout(explicit).gap(), 32);
        assert_eq!(document.computed_style_for_layout(invalid).gap(), 0);
    }

    #[test]
    fn justify_content_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { justify-content: space-between; } #explicit { justify-content: flex-end; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='justify-content: center'>Explicit</span><span id='invalid' style='justify-content: space-around'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).justify_content(),
            JustifyContentValue::SpaceBetween
        );
        assert_eq!(
            document.computed_style_for_layout(child).justify_content(),
            JustifyContentValue::FlexStart
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .justify_content(),
            JustifyContentValue::Center
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .justify_content(),
            JustifyContentValue::FlexStart
        );
    }

    #[test]
    fn color_parser_accepts_bounded_forms_only() {
        assert_eq!(
            parse_color("#abc"),
            Some(NativeColor {
                red: 170,
                green: 187,
                blue: 204,
                alpha: 255,
            })
        );
        assert_eq!(
            parse_color("#10203080"),
            Some(NativeColor {
                red: 16,
                green: 32,
                blue: 48,
                alpha: 128,
            })
        );
        assert_eq!(
            parse_color("rgb(1, 2, 3)"),
            Some(NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            })
        );
        assert_eq!(
            parse_color("rgba(1, 2, 3, 0.5)"),
            Some(NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 128,
            })
        );
        assert_eq!(parse_color("rgb(101%, 2, 3)"), None);
    }
}
