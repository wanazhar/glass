use super::config::{MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_VIEWPORT_DIMENSION};
use super::diagnostics::{NativeDiagnosticCode, NativeDiagnosticSink, NativeDiagnosticSource};
use super::dom::{NativeDocument, NativeNode, NativeNodeId};
use super::error::NativeEngineError;

pub(crate) const MAX_NATIVE_STYLE_RULES: usize = 512;
pub(crate) const MIN_NATIVE_FLEX_ITEM_ORDER: i32 = -1024;
pub(crate) const MAX_NATIVE_FLEX_ITEM_ORDER: i32 = 1024;
pub(crate) const MAX_NATIVE_FLEX_GROW: u32 = 1024;
pub(crate) const MAX_NATIVE_FLEX_SHRINK: u32 = 1024;
pub(crate) const MAX_NATIVE_TEXT_DECORATION_THICKNESS: u32 = 4;
pub(crate) const MIN_NATIVE_TEXT_UNDERLINE_OFFSET: i32 = -4;
pub(crate) const MAX_NATIVE_TEXT_UNDERLINE_OFFSET: i32 = 4;
const MAX_SELECTOR_BYTES: usize = 256;
const MAX_SELECTOR_PARTS: usize = 8;
const MAX_NATIVE_NAMED_CASCADE_LAYERS: usize = 15;
const UNLAYERED_CASCADE_LAYER: u16 = MAX_NATIVE_NAMED_CASCADE_LAYERS as u16;
const CASCADE_SPECIFICITY_BITS: u32 = 12;
const CASCADE_SPECIFICITY_STRIDE: u16 = 1 << CASCADE_SPECIFICITY_BITS;
const MAX_NATIVE_SELECTOR_SPECIFICITY: u16 = CASCADE_SPECIFICITY_STRIDE - 1;
const MAX_NATIVE_CASCADE_LAYERS: usize = MAX_NATIVE_NAMED_CASCADE_LAYERS + 1;
const CASCADE_DECLARATION_ORDER_STRIDE: usize = 256 * 1024 + 1;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeBorderStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBorderStyleValue {
    Paint(NativeBorderStyle),
    None,
    Hidden,
}

/// Bounded text-decoration patterns owned separately from border styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTextDecorationStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Double,
    Wavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationStyleDeclaration {
    Value(NativeTextDecorationStyle),
    RevertLayer,
}

impl NativeTextDecorationStyleDeclaration {
    const fn resolve(self, inherited: NativeTextDecorationStyle) -> NativeTextDecorationStyle {
        match self {
            Self::Value(value) => value,
            Self::RevertLayer => inherited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationThicknessDeclaration {
    Value(u32),
    RevertLayer,
}

impl NativeTextDecorationThicknessDeclaration {
    const fn resolve(self, inherited: u32) -> u32 {
        match self {
            Self::Value(value) => value,
            Self::RevertLayer => inherited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextUnderlineOffsetDeclaration {
    Value(i32),
    RevertLayer,
}

impl NativeTextUnderlineOffsetDeclaration {
    const fn resolve(self, inherited: i32) -> i32 {
        match self {
            Self::Value(value) => value,
            Self::RevertLayer => inherited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationColorDeclaration {
    Value(NativeColor),
    RevertLayer,
}

/// Bounded inherited glyph-intersection behavior for text decorations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTextDecorationSkipInk {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationSkipInkDeclaration {
    Value(NativeTextDecorationSkipInk),
    RevertLayer,
}

impl NativeTextDecorationSkipInkDeclaration {
    const fn resolve(self, inherited: NativeTextDecorationSkipInk) -> NativeTextDecorationSkipInk {
        match self {
            Self::Value(value) => value,
            Self::RevertLayer => inherited,
        }
    }
}

/// Bounded inherited fixed-cell whitespace behavior for text decorations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTextDecorationSkipSpaces {
    #[default]
    None,
    All,
    Start,
    End,
    StartAndEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationSkipSpacesDeclaration {
    Value(NativeTextDecorationSkipSpaces),
    Inherit,
    Unset,
    Revert,
    RevertLayer,
}

impl NativeTextDecorationSkipSpacesDeclaration {
    const fn resolve(
        self,
        inherited: NativeTextDecorationSkipSpaces,
    ) -> NativeTextDecorationSkipSpaces {
        match self {
            Self::Value(value) => value,
            Self::Inherit | Self::Unset | Self::Revert | Self::RevertLayer => inherited,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBorderDeclaration {
    Complete(NativeBorderSide),
    None,
    Hidden,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhiteSpaceDeclaration {
    Value(WhiteSpaceValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineHeightDeclaration {
    Value(u32),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextAlignValue {
    #[default]
    Left,
    Center,
    Right,
    Start,
    End,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextAlignDeclaration {
    Value(TextAlignValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextAlignLastValue {
    #[default]
    Auto,
    Left,
    Center,
    Right,
    Start,
    End,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextAlignLastDeclaration {
    Value(TextAlignLastValue),
    RevertLayer,
}

impl TextAlignLastValue {
    pub(crate) const fn resolve(self, text_align: TextAlignValue) -> TextAlignValue {
        match self {
            Self::Auto => match text_align {
                TextAlignValue::Justify => TextAlignValue::Left,
                value => value,
            },
            Self::Left => TextAlignValue::Left,
            Self::Center => TextAlignValue::Center,
            Self::Right => TextAlignValue::Right,
            Self::Start => TextAlignValue::Start,
            Self::End => TextAlignValue::End,
            Self::Justify => TextAlignValue::Justify,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TextJustifyValue {
    #[default]
    Auto,
    None,
    InterWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextJustifyDeclaration {
    Value(TextJustifyValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum JustifyContentValue {
    #[default]
    FlexStart,
    Normal,
    Stretch,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JustifyContentDeclaration {
    Value(JustifyContentValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AlignItemsValue {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    Stretch,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignItemsDeclaration {
    Value(AlignItemsValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AlignSelfValue {
    #[default]
    Auto,
    FlexStart,
    Center,
    FlexEnd,
    Stretch,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignSelfDeclaration {
    Value(AlignSelfValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AlignContentValue {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignContentDeclaration {
    Value(AlignContentValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FlexDirectionValue {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexDirectionDeclaration {
    Value(FlexDirectionValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum DirectionValue {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectionDeclaration {
    Value(DirectionValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FlexWrapValue {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexWrapDeclaration {
    Value(FlexWrapValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum FlexBasisValue {
    #[default]
    Auto,
    Length(u32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeOrderValue(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexItemOrderDeclaration {
    Value(NativeOrderValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexGrowDeclaration {
    Value(u32),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexShrinkDeclaration {
    Value(u32),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexBasisDeclaration {
    Value(FlexBasisValue),
    RevertLayer,
}

impl NativeOrderValue {
    pub(crate) const fn value(self) -> i32 {
        self.0
    }
}

/// The bounded line decorations supported by the native renderer.
///
/// The bit representation keeps the computed value compact while preserving
/// independent inheritance and painting state for each supported line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TextDecorationValue(u8);

impl TextDecorationValue {
    const UNDERLINE: u8 = 1 << 0;
    const OVERLINE: u8 = 1 << 1;
    const LINE_THROUGH: u8 = 1 << 2;

    pub(crate) const fn none() -> Self {
        Self(0)
    }

    pub(crate) const fn new(underline: bool, overline: bool, line_through: bool) -> Self {
        Self(
            (if underline { Self::UNDERLINE } else { 0 })
                | (if overline { Self::OVERLINE } else { 0 })
                | (if line_through { Self::LINE_THROUGH } else { 0 }),
        )
    }

    pub(crate) const fn underline(self) -> bool {
        self.0 & Self::UNDERLINE != 0
    }

    pub(crate) const fn overline(self) -> bool {
        self.0 & Self::OVERLINE != 0
    }

    pub(crate) const fn line_through(self) -> bool {
        self.0 & Self::LINE_THROUGH != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationDeclaration {
    Value(TextDecorationValue),
    RevertLayer,
}

impl NativeTextDecorationDeclaration {
    const fn resolve(self, inherited: TextDecorationValue) -> TextDecorationValue {
        match self {
            Self::Value(value) => value,
            Self::RevertLayer => inherited,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InheritedTextDeclaration<T> {
    Value(T),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalCascadeDeclaration<T> {
    Value(T),
    RevertLayer,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeGapValue {
    row: u32,
    column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapShorthandDeclaration {
    Value(NativeGapValue),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapComponentDeclaration {
    Value(u32),
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeMarginValue {
    Length(u32),
    Auto,
}

impl NativeMarginValue {
    const fn length(self) -> u32 {
        match self {
            Self::Length(value) => value,
            Self::Auto => 0,
        }
    }

    const fn is_auto(self) -> bool {
        matches!(self, Self::Auto)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeInheritedStyle {
    pub(crate) color: Option<NativeColor>,
    pub(crate) direction: DirectionValue,
    pub(crate) white_space: WhiteSpaceValue,
    pub(crate) line_height: Option<u32>,
    pub(crate) text_align: TextAlignValue,
    pub(crate) text_align_last: TextAlignLastValue,
    pub(crate) text_justify: TextJustifyValue,
    pub(crate) text_decoration: TextDecorationValue,
    pub(crate) text_decoration_style: NativeTextDecorationStyle,
    pub(crate) text_decoration_skip_ink: NativeTextDecorationSkipInk,
    pub(crate) text_decoration_skip_spaces: NativeTextDecorationSkipSpaces,
    pub(crate) text_decoration_thickness: u32,
    pub(crate) text_underline_offset: i32,
    pub(crate) text_transform: TextTransformValue,
    pub(crate) font_weight: FontWeightValue,
    pub(crate) font_style: FontStyleValue,
    pub(crate) word_break: WordBreakValue,
    pub(crate) vertical_align: VerticalAlignValue,
    pub(crate) word_spacing: u32,
    pub(crate) letter_spacing: u32,
}

impl Default for NativeInheritedStyle {
    fn default() -> Self {
        Self {
            color: None,
            direction: DirectionValue::Ltr,
            white_space: WhiteSpaceValue::Normal,
            line_height: None,
            text_align: TextAlignValue::Left,
            text_align_last: TextAlignLastValue::Auto,
            text_justify: TextJustifyValue::Auto,
            text_decoration: TextDecorationValue::none(),
            text_decoration_style: NativeTextDecorationStyle::Solid,
            text_decoration_skip_ink: NativeTextDecorationSkipInk::Auto,
            text_decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            text_decoration_thickness: 1,
            text_underline_offset: 0,
            text_transform: TextTransformValue::None,
            font_weight: FontWeightValue::Normal,
            font_style: FontStyleValue::Normal,
            word_break: WordBreakValue::Normal,
            vertical_align: VerticalAlignValue::Baseline,
            word_spacing: 0,
            letter_spacing: 0,
        }
    }
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
    fn from_values(values: [Option<u32>; 4]) -> Self {
        Self {
            top: values[0].unwrap_or(0),
            right: values[1].unwrap_or(0),
            bottom: values[2].unwrap_or(0),
            left: values[3].unwrap_or(0),
        }
    }

    fn from_margin_values(values: [Option<NativeMarginValue>; 4]) -> Self {
        Self {
            top: values[0].map_or(0, NativeMarginValue::length),
            right: values[1].map_or(0, NativeMarginValue::length),
            bottom: values[2].map_or(0, NativeMarginValue::length),
            left: values[3].map_or(0, NativeMarginValue::length),
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

    pub(crate) const fn with_top(self, top: u32) -> Self {
        Self { top, ..self }
    }

    pub(crate) const fn with_right(self, right: u32) -> Self {
        Self { right, ..self }
    }

    pub(crate) const fn with_bottom(self, bottom: u32) -> Self {
        Self { bottom, ..self }
    }

    pub(crate) const fn with_left(self, left: u32) -> Self {
        Self { left, ..self }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeAutoEdges {
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
}

impl NativeAutoEdges {
    fn from_values(values: [Option<NativeMarginValue>; 4]) -> Self {
        Self {
            top: values[0].is_some_and(NativeMarginValue::is_auto),
            right: values[1].is_some_and(NativeMarginValue::is_auto),
            bottom: values[2].is_some_and(NativeMarginValue::is_auto),
            left: values[3].is_some_and(NativeMarginValue::is_auto),
        }
    }

    pub(crate) const fn top(self) -> bool {
        self.top
    }

    pub(crate) const fn right(self) -> bool {
        self.right
    }

    pub(crate) const fn bottom(self) -> bool {
        self.bottom
    }

    pub(crate) const fn left(self) -> bool {
        self.left
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeComputedStyle {
    display: DisplayValue,
    visibility_hidden: bool,
    opacity: Option<u8>,
    white_space: WhiteSpaceValue,
    text_align: TextAlignValue,
    text_align_last: TextAlignLastValue,
    text_justify: TextJustifyValue,
    justify_content: JustifyContentValue,
    align_items: AlignItemsValue,
    align_self: AlignSelfValue,
    align_content: AlignContentValue,
    flex_direction: FlexDirectionValue,
    direction: DirectionValue,
    flex_wrap: FlexWrapValue,
    flex_item_order: NativeOrderValue,
    flex_grow: u32,
    flex_shrink: u32,
    flex_basis: FlexBasisValue,
    text_decoration: TextDecorationValue,
    text_decoration_style: NativeTextDecorationStyle,
    text_decoration_skip_ink: NativeTextDecorationSkipInk,
    text_decoration_skip_spaces: NativeTextDecorationSkipSpaces,
    text_decoration_thickness: u32,
    text_underline_offset: i32,
    text_decoration_color: Option<NativeColor>,
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
    row_gap: u32,
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
    margin_auto: NativeAutoEdges,
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

    pub(crate) const fn margin_auto(self) -> NativeAutoEdges {
        self.margin_auto
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
        let mut layers = Vec::new();
        for (index, source) in sources.into_iter().enumerate() {
            parse_source(
                &source,
                &mut stylesheet.rules,
                &mut order,
                NativeDiagnosticSource::Stylesheet { index },
                diagnostics,
                &mut layers,
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
        let mut display: [Option<CascadeValue<LocalCascadeDeclaration<DisplayValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut visibility: [Option<CascadeValue<LocalCascadeDeclaration<VisibilityValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut opacity: [Option<CascadeValue<LocalCascadeDeclaration<u8>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut white_space: [Option<CascadeValue<WhiteSpaceDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_align: [Option<CascadeValue<TextAlignDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_align_last: [Option<CascadeValue<TextAlignLastDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_justify: [Option<CascadeValue<TextJustifyDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut direction: [Option<CascadeValue<DirectionDeclaration>>; MAX_NATIVE_CASCADE_LAYERS] =
            [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut justify_content: [Option<CascadeValue<JustifyContentDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut align_items: [Option<CascadeValue<AlignItemsDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut align_self: [Option<CascadeValue<AlignSelfDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut align_content: [Option<CascadeValue<AlignContentDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut flex_direction: [Option<CascadeValue<FlexDirectionDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut flex_wrap: [Option<CascadeValue<FlexWrapDeclaration>>; MAX_NATIVE_CASCADE_LAYERS] =
            [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut flex_item_order: [Option<CascadeValue<FlexItemOrderDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut flex_grow: [Option<CascadeValue<FlexGrowDeclaration>>; MAX_NATIVE_CASCADE_LAYERS] =
            [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut flex_shrink: [Option<CascadeValue<FlexShrinkDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut flex_basis: [Option<CascadeValue<FlexBasisDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_decoration: [Option<CascadeValue<NativeTextDecorationDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_decoration_style: [Option<CascadeValue<NativeTextDecorationStyleDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_decoration_skip_ink: [Option<
            CascadeValue<NativeTextDecorationSkipInkDeclaration>,
        >; MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_decoration_skip_spaces: [Option<
            CascadeValue<NativeTextDecorationSkipSpacesDeclaration>,
        >; MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_decoration_thickness: [Option<
            CascadeValue<NativeTextDecorationThicknessDeclaration>,
        >; MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_underline_offset: [Option<CascadeValue<NativeTextUnderlineOffsetDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_decoration_color: [Option<CascadeValue<NativeTextDecorationColorDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_transform: [Option<CascadeValue<InheritedTextDeclaration<TextTransformValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut font_weight: [Option<CascadeValue<InheritedTextDeclaration<FontWeightValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut font_style: [Option<CascadeValue<InheritedTextDeclaration<FontStyleValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut word_break: [Option<CascadeValue<InheritedTextDeclaration<WordBreakValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_overflow: [Option<CascadeValue<LocalCascadeDeclaration<TextOverflowValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut vertical_align: [Option<CascadeValue<InheritedTextDeclaration<VerticalAlignValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut text_indent: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut word_spacing: [Option<CascadeValue<InheritedTextDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut letter_spacing: [Option<CascadeValue<InheritedTextDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut gap = GapCascade::default();
        let mut width: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut height: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut min_width: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut max_width: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut min_height: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut max_height: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut line_height: [Option<CascadeValue<LineHeightDeclaration>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut background_color: [Option<CascadeValue<LocalCascadeDeclaration<NativeColor>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut border: [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderDeclaration>>>;
            MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut border_width: [[Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut border_style: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>,
        >; MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut border_color: [[Option<CascadeValue<LocalCascadeDeclaration<NativeColor>>>;
            MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut border_radius: [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadius>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut padding: [[Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut margin: [[Option<CascadeValue<LocalCascadeDeclaration<NativeMarginValue>>>;
            MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut box_sizing: [Option<CascadeValue<LocalCascadeDeclaration<NativeBoxSizing>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut color: [Option<CascadeValue<LocalCascadeDeclaration<NativeColor>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut overflow_x: [Option<CascadeValue<LocalCascadeDeclaration<OverflowValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        let mut overflow_y: [Option<CascadeValue<LocalCascadeDeclaration<OverflowValue>>>;
            MAX_NATIVE_CASCADE_LAYERS] = [None; MAX_NATIVE_CASCADE_LAYERS];
        for rule in &self.rules {
            if !matches(&rule.selector) {
                continue;
            }
            apply_local_cascade_declaration(
                rule.declarations.display,
                rule.selector.specificity,
                rule.order,
                false,
                &mut display,
            );
            apply_local_cascade_declaration(
                rule.declarations.visibility,
                rule.selector.specificity,
                rule.order,
                false,
                &mut visibility,
            );
            apply_local_cascade_declaration(
                rule.declarations.opacity,
                rule.selector.specificity,
                rule.order,
                false,
                &mut opacity,
            );
            let layer = cascade_layer_index(rule.selector.specificity);
            if let Some(value) = rule.declarations.white_space
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    white_space[layer],
                )
            {
                white_space[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_align
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_align[layer],
                )
            {
                text_align[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_align_last
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_align_last[layer],
                )
            {
                text_align_last[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_justify
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_justify[layer],
                )
            {
                text_justify[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.direction
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    direction[layer],
                )
            {
                direction[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.text_decoration {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration[layer],
                ) {
                    text_decoration[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            if let Some(value) = rule.declarations.text_decoration_style {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration_style[layer],
                ) {
                    text_decoration_style[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            if let Some(value) = rule.declarations.text_decoration_skip_ink {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration_skip_ink[layer],
                ) {
                    text_decoration_skip_ink[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            if let Some(value) = rule.declarations.text_decoration_skip_spaces {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration_skip_spaces[layer],
                ) {
                    text_decoration_skip_spaces[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            if let Some(value) = rule.declarations.text_decoration_thickness {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration_thickness[layer],
                ) {
                    text_decoration_thickness[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            if let Some(value) = rule.declarations.text_underline_offset {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_underline_offset[layer],
                ) {
                    text_underline_offset[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            if let Some(value) = rule.declarations.text_decoration_color {
                let layer = cascade_layer_index(rule.selector.specificity);
                if wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    text_decoration_color[layer],
                ) {
                    text_decoration_color[layer] = Some(CascadeValue {
                        value,
                        specificity: rule.selector.specificity,
                        order: rule.order,
                        inline: false,
                    });
                }
            }
            apply_inherited_text_declaration(
                rule.declarations.text_transform,
                rule.selector.specificity,
                rule.order,
                false,
                &mut text_transform,
            );
            apply_inherited_text_declaration(
                rule.declarations.font_weight,
                rule.selector.specificity,
                rule.order,
                false,
                &mut font_weight,
            );
            apply_inherited_text_declaration(
                rule.declarations.font_style,
                rule.selector.specificity,
                rule.order,
                false,
                &mut font_style,
            );
            apply_inherited_text_declaration(
                rule.declarations.word_break,
                rule.selector.specificity,
                rule.order,
                false,
                &mut word_break,
            );
            apply_local_cascade_declaration(
                rule.declarations.text_overflow,
                rule.selector.specificity,
                rule.order,
                false,
                &mut text_overflow,
            );
            apply_inherited_text_declaration(
                rule.declarations.vertical_align,
                rule.selector.specificity,
                rule.order,
                false,
                &mut vertical_align,
            );
            apply_local_cascade_declaration(
                rule.declarations.text_indent,
                rule.selector.specificity,
                rule.order,
                false,
                &mut text_indent,
            );
            apply_inherited_text_declaration(
                rule.declarations.word_spacing,
                rule.selector.specificity,
                rule.order,
                false,
                &mut word_spacing,
            );
            apply_inherited_text_declaration(
                rule.declarations.letter_spacing,
                rule.selector.specificity,
                rule.order,
                false,
                &mut letter_spacing,
            );
            apply_gap_declarations(
                rule.declarations,
                rule.selector.specificity,
                rule.order,
                false,
                &mut gap,
            );
            if let Some(value) = rule.declarations.justify_content
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    justify_content[layer],
                )
            {
                justify_content[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.order
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    flex_item_order[layer],
                )
            {
                flex_item_order[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.flex_grow
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    flex_grow[layer],
                )
            {
                flex_grow[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.flex_shrink
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    flex_shrink[layer],
                )
            {
                flex_shrink[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.flex_basis
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    flex_basis[layer],
                )
            {
                flex_basis[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.align_items
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    align_items[layer],
                )
            {
                align_items[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.align_self
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    align_self[layer],
                )
            {
                align_self[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.align_content
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    align_content[layer],
                )
            {
                align_content[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.flex_direction
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    flex_direction[layer],
                )
            {
                flex_direction[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            if let Some(value) = rule.declarations.flex_wrap
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    flex_wrap[layer],
                )
            {
                flex_wrap[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            apply_local_cascade_declaration(
                rule.declarations.width,
                rule.selector.specificity,
                rule.order,
                false,
                &mut width,
            );
            apply_local_cascade_declaration(
                rule.declarations.height,
                rule.selector.specificity,
                rule.order,
                false,
                &mut height,
            );
            apply_local_cascade_declaration(
                rule.declarations.min_width,
                rule.selector.specificity,
                rule.order,
                false,
                &mut min_width,
            );
            apply_local_cascade_declaration(
                rule.declarations.max_width,
                rule.selector.specificity,
                rule.order,
                false,
                &mut max_width,
            );
            apply_local_cascade_declaration(
                rule.declarations.min_height,
                rule.selector.specificity,
                rule.order,
                false,
                &mut min_height,
            );
            apply_local_cascade_declaration(
                rule.declarations.max_height,
                rule.selector.specificity,
                rule.order,
                false,
                &mut max_height,
            );
            if let Some(value) = rule.declarations.line_height
                && wins(
                    rule.selector.specificity,
                    rule.order,
                    false,
                    line_height[layer],
                )
            {
                line_height[layer] = Some(CascadeValue {
                    value,
                    specificity: rule.selector.specificity,
                    order: rule.order,
                    inline: false,
                });
            }
            apply_local_cascade_declaration(
                rule.declarations.background_color,
                rule.selector.specificity,
                rule.order,
                false,
                &mut background_color,
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
            apply_local_cascade_declaration(
                rule.declarations.border_radius,
                rule.selector.specificity,
                rule.order,
                false,
                &mut border_radius,
            );
            apply_local_cascade_edges(
                &rule.declarations.padding,
                rule.selector.specificity,
                rule.order,
                false,
                &mut padding,
            );
            apply_local_cascade_edges(
                &rule.declarations.margin,
                rule.selector.specificity,
                rule.order,
                false,
                &mut margin,
            );
            apply_local_cascade_declaration(
                rule.declarations.box_sizing,
                rule.selector.specificity,
                rule.order,
                false,
                &mut box_sizing,
            );
            apply_local_cascade_declaration(
                rule.declarations.color,
                rule.selector.specificity,
                rule.order,
                false,
                &mut color,
            );
            apply_local_cascade_declaration(
                rule.declarations.overflow_x,
                rule.selector.specificity,
                rule.order,
                false,
                &mut overflow_x,
            );
            apply_local_cascade_declaration(
                rule.declarations.overflow_y,
                rule.selector.specificity,
                rule.order,
                false,
                &mut overflow_y,
            );
        }

        if let Some(inline_style) = node.attribute("style") {
            let declarations = parse_declarations(inline_style);
            apply_local_cascade_declaration(
                declarations.display,
                u16::MAX,
                usize::MAX,
                true,
                &mut display,
            );
            apply_local_cascade_declaration(
                declarations.visibility,
                u16::MAX,
                usize::MAX,
                true,
                &mut visibility,
            );
            apply_local_cascade_declaration(
                declarations.opacity,
                u16::MAX,
                usize::MAX,
                true,
                &mut opacity,
            );
            let layer = usize::from(UNLAYERED_CASCADE_LAYER);
            if let Some(value) = declarations.white_space
                && wins(u16::MAX, usize::MAX, true, white_space[layer])
            {
                white_space[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_align
                && wins(u16::MAX, usize::MAX, true, text_align[layer])
            {
                text_align[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_align_last
                && wins(u16::MAX, usize::MAX, true, text_align_last[layer])
            {
                text_align_last[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_justify
                && wins(u16::MAX, usize::MAX, true, text_justify[layer])
            {
                text_justify[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.direction
                && wins(u16::MAX, usize::MAX, true, direction[layer])
            {
                direction[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.text_decoration {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(u16::MAX, usize::MAX, true, text_decoration[layer]) {
                    text_decoration[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            if let Some(value) = declarations.text_decoration_style {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(u16::MAX, usize::MAX, true, text_decoration_style[layer]) {
                    text_decoration_style[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            if let Some(value) = declarations.text_decoration_skip_ink {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(u16::MAX, usize::MAX, true, text_decoration_skip_ink[layer]) {
                    text_decoration_skip_ink[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            if let Some(value) = declarations.text_decoration_skip_spaces {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(
                    u16::MAX,
                    usize::MAX,
                    true,
                    text_decoration_skip_spaces[layer],
                ) {
                    text_decoration_skip_spaces[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            if let Some(value) = declarations.text_decoration_thickness {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(u16::MAX, usize::MAX, true, text_decoration_thickness[layer]) {
                    text_decoration_thickness[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            if let Some(value) = declarations.text_underline_offset {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(u16::MAX, usize::MAX, true, text_underline_offset[layer]) {
                    text_underline_offset[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            if let Some(value) = declarations.text_decoration_color {
                let layer = usize::from(UNLAYERED_CASCADE_LAYER);
                if wins(u16::MAX, usize::MAX, true, text_decoration_color[layer]) {
                    text_decoration_color[layer] = Some(CascadeValue {
                        value,
                        specificity: u16::MAX,
                        order: usize::MAX,
                        inline: true,
                    });
                }
            }
            apply_inherited_text_declaration(
                declarations.text_transform,
                u16::MAX,
                usize::MAX,
                true,
                &mut text_transform,
            );
            apply_inherited_text_declaration(
                declarations.font_weight,
                u16::MAX,
                usize::MAX,
                true,
                &mut font_weight,
            );
            apply_inherited_text_declaration(
                declarations.font_style,
                u16::MAX,
                usize::MAX,
                true,
                &mut font_style,
            );
            apply_inherited_text_declaration(
                declarations.word_break,
                u16::MAX,
                usize::MAX,
                true,
                &mut word_break,
            );
            apply_local_cascade_declaration(
                declarations.text_overflow,
                u16::MAX,
                usize::MAX,
                true,
                &mut text_overflow,
            );
            apply_inherited_text_declaration(
                declarations.vertical_align,
                u16::MAX,
                usize::MAX,
                true,
                &mut vertical_align,
            );
            apply_local_cascade_declaration(
                declarations.text_indent,
                u16::MAX,
                usize::MAX,
                true,
                &mut text_indent,
            );
            apply_inherited_text_declaration(
                declarations.word_spacing,
                u16::MAX,
                usize::MAX,
                true,
                &mut word_spacing,
            );
            apply_inherited_text_declaration(
                declarations.letter_spacing,
                u16::MAX,
                usize::MAX,
                true,
                &mut letter_spacing,
            );
            apply_gap_declarations(declarations, u16::MAX, usize::MAX, true, &mut gap);
            if let Some(value) = declarations.justify_content
                && wins(u16::MAX, usize::MAX, true, justify_content[layer])
            {
                justify_content[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.order
                && wins(u16::MAX, usize::MAX, true, flex_item_order[layer])
            {
                flex_item_order[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.flex_grow
                && wins(u16::MAX, usize::MAX, true, flex_grow[layer])
            {
                flex_grow[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.flex_shrink
                && wins(u16::MAX, usize::MAX, true, flex_shrink[layer])
            {
                flex_shrink[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.flex_basis
                && wins(u16::MAX, usize::MAX, true, flex_basis[layer])
            {
                flex_basis[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.align_items
                && wins(u16::MAX, usize::MAX, true, align_items[layer])
            {
                align_items[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.align_self
                && wins(u16::MAX, usize::MAX, true, align_self[layer])
            {
                align_self[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.align_content
                && wins(u16::MAX, usize::MAX, true, align_content[layer])
            {
                align_content[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.flex_direction
                && wins(u16::MAX, usize::MAX, true, flex_direction[layer])
            {
                flex_direction[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            if let Some(value) = declarations.flex_wrap
                && wins(u16::MAX, usize::MAX, true, flex_wrap[layer])
            {
                flex_wrap[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            apply_local_cascade_declaration(
                declarations.width,
                u16::MAX,
                usize::MAX,
                true,
                &mut width,
            );
            apply_local_cascade_declaration(
                declarations.height,
                u16::MAX,
                usize::MAX,
                true,
                &mut height,
            );
            apply_local_cascade_declaration(
                declarations.min_width,
                u16::MAX,
                usize::MAX,
                true,
                &mut min_width,
            );
            apply_local_cascade_declaration(
                declarations.max_width,
                u16::MAX,
                usize::MAX,
                true,
                &mut max_width,
            );
            apply_local_cascade_declaration(
                declarations.min_height,
                u16::MAX,
                usize::MAX,
                true,
                &mut min_height,
            );
            apply_local_cascade_declaration(
                declarations.max_height,
                u16::MAX,
                usize::MAX,
                true,
                &mut max_height,
            );
            if let Some(value) = declarations.line_height
                && wins(u16::MAX, usize::MAX, true, line_height[layer])
            {
                line_height[layer] = Some(CascadeValue {
                    value,
                    specificity: u16::MAX,
                    order: usize::MAX,
                    inline: true,
                });
            }
            apply_local_cascade_declaration(
                declarations.background_color,
                u16::MAX,
                usize::MAX,
                true,
                &mut background_color,
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
            apply_local_cascade_declaration(
                declarations.border_radius,
                u16::MAX,
                usize::MAX,
                true,
                &mut border_radius,
            );
            apply_local_cascade_edges(
                &declarations.padding,
                u16::MAX,
                usize::MAX,
                true,
                &mut padding,
            );
            apply_local_cascade_edges(
                &declarations.margin,
                u16::MAX,
                usize::MAX,
                true,
                &mut margin,
            );
            apply_local_cascade_declaration(
                declarations.box_sizing,
                u16::MAX,
                usize::MAX,
                true,
                &mut box_sizing,
            );
            apply_local_cascade_declaration(
                declarations.color,
                u16::MAX,
                usize::MAX,
                true,
                &mut color,
            );
            apply_local_cascade_declaration(
                declarations.overflow_x,
                u16::MAX,
                usize::MAX,
                true,
                &mut overflow_x,
            );
            apply_local_cascade_declaration(
                declarations.overflow_y,
                u16::MAX,
                usize::MAX,
                true,
                &mut overflow_y,
            );
        }

        let resolved_padding = padding.map(resolve_local_optional_cascade_declaration);
        let resolved_margin = margin.map(resolve_local_optional_cascade_declaration);
        let resolved_border = border.map(resolve_local_optional_cascade_declaration);
        let resolved_border_width: [Option<u32>; 4] = std::array::from_fn(|index| {
            resolve_local_optional_cascade_declaration(border_width[index])
        });
        let resolved_border_style: [Option<NativeBorderStyleValue>; 4] =
            std::array::from_fn(|index| {
                resolve_local_optional_cascade_declaration(border_style[index])
            });
        let resolved_border_color = border_color.map(resolve_local_optional_cascade_declaration);
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

        NativeComputedStyle {
            display: resolve_local_cascade_declaration(display, DisplayValue::Auto),
            visibility_hidden: resolve_local_cascade_declaration(
                visibility,
                VisibilityValue::Other,
            ) == VisibilityValue::Hidden,
            opacity: resolve_local_optional_cascade_declaration(opacity),
            white_space: resolve_white_space(white_space, inherited.white_space),
            text_align: resolve_text_align(text_align, inherited.text_align),
            text_align_last: resolve_text_align_last(text_align_last, inherited.text_align_last),
            text_justify: resolve_text_justify(text_justify, inherited.text_justify),
            justify_content: resolve_justify_content(justify_content),
            align_items: resolve_align_items(align_items),
            align_self: resolve_align_self(align_self),
            align_content: resolve_align_content(align_content),
            flex_direction: resolve_flex_direction(flex_direction),
            direction: resolve_direction(direction, inherited.direction),
            flex_wrap: resolve_flex_wrap(flex_wrap),
            flex_item_order: resolve_flex_item_order(flex_item_order),
            flex_grow: resolve_flex_grow(flex_grow),
            flex_shrink: resolve_flex_shrink(flex_shrink),
            flex_basis: resolve_flex_basis(flex_basis),
            text_decoration: resolve_text_decoration(text_decoration, inherited.text_decoration),
            text_decoration_style: resolve_text_decoration_style(
                text_decoration_style,
                inherited.text_decoration_style,
            ),
            text_decoration_skip_ink: resolve_text_decoration_skip_ink(
                text_decoration_skip_ink,
                inherited.text_decoration_skip_ink,
            ),
            text_decoration_skip_spaces: resolve_text_decoration_skip_spaces(
                text_decoration_skip_spaces,
                inherited.text_decoration_skip_spaces,
            ),
            text_decoration_thickness: resolve_text_decoration_thickness(
                text_decoration_thickness,
                inherited.text_decoration_thickness,
            ),
            text_underline_offset: resolve_text_underline_offset(
                text_underline_offset,
                inherited.text_underline_offset,
            ),
            text_decoration_color: resolve_text_decoration_color(text_decoration_color),
            text_transform: resolve_inherited_text_declaration(
                text_transform,
                inherited.text_transform,
            ),
            font_weight: resolve_inherited_text_declaration(font_weight, inherited.font_weight),
            font_style: resolve_inherited_text_declaration(font_style, inherited.font_style),
            word_break: resolve_inherited_text_declaration(word_break, inherited.word_break),
            text_overflow: resolve_local_cascade_declaration(
                text_overflow,
                TextOverflowValue::Clip,
            ),
            vertical_align: resolve_inherited_text_declaration(
                vertical_align,
                inherited.vertical_align,
            ),
            text_indent: resolve_local_cascade_declaration(text_indent, 0),
            word_spacing: resolve_inherited_text_declaration(word_spacing, inherited.word_spacing),
            letter_spacing: resolve_inherited_text_declaration(
                letter_spacing,
                inherited.letter_spacing,
            ),
            gap: resolve_gap_axis(gap.shorthand_column, gap.column_gap),
            row_gap: resolve_gap_axis(gap.shorthand_row, gap.row_gap),
            width: resolve_local_optional_cascade_declaration(width),
            height: resolve_local_optional_cascade_declaration(height),
            min_width: resolve_local_optional_cascade_declaration(min_width),
            max_width: resolve_local_optional_cascade_declaration(max_width),
            min_height: resolve_local_optional_cascade_declaration(min_height),
            max_height: resolve_local_optional_cascade_declaration(max_height),
            line_height: resolve_line_height(line_height, inherited.line_height),
            background_color: resolve_local_optional_cascade_declaration(background_color),
            border: NativeBorder::from_sides(resolved_border),
            border_radius: resolve_local_cascade_declaration(
                border_radius,
                NativeBorderRadius::default(),
            ),
            padding: NativeBoxEdges::from_values(resolved_padding),
            margin: NativeBoxEdges::from_margin_values(resolved_margin),
            margin_auto: NativeAutoEdges::from_values(resolved_margin),
            box_sizing: resolve_local_cascade_declaration(box_sizing, NativeBoxSizing::ContentBox),
            color: resolve_local_optional_cascade_declaration(color).or(inherited.color),
            overflow_clip_x: resolve_local_optional_cascade_declaration(overflow_x)
                .is_some_and(|value| matches!(value, OverflowValue::Hidden | OverflowValue::Clip)),
            overflow_clip_y: resolve_local_optional_cascade_declaration(overflow_y)
                .is_some_and(|value| matches!(value, OverflowValue::Hidden | OverflowValue::Clip)),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GapCascadeValue<T> {
    value: T,
    specificity: u16,
    order: usize,
    declaration_order: usize,
    inline: bool,
}

#[derive(Debug, Clone, Copy)]
struct GapCascade {
    shorthand_row: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    shorthand_column: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    row_gap: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    column_gap: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
}

impl Default for GapCascade {
    fn default() -> Self {
        Self {
            shorthand_row: [None; MAX_NATIVE_CASCADE_LAYERS],
            shorthand_column: [None; MAX_NATIVE_CASCADE_LAYERS],
            row_gap: [None; MAX_NATIVE_CASCADE_LAYERS],
            column_gap: [None; MAX_NATIVE_CASCADE_LAYERS],
        }
    }
}

fn wins<T>(specificity: u16, order: usize, inline: bool, current: Option<CascadeValue<T>>) -> bool {
    current.is_none_or(|current| {
        (inline, specificity, order) > (current.inline, current.specificity, current.order)
    })
}

fn cascade_layer_index(specificity: u16) -> usize {
    usize::from(specificity / CASCADE_SPECIFICITY_STRIDE).min(usize::from(UNLAYERED_CASCADE_LAYER))
}

fn encode_cascade_specificity(specificity: u16, layer: Option<usize>) -> u16 {
    let layer = layer
        .and_then(|layer| u16::try_from(layer).ok())
        .unwrap_or(UNLAYERED_CASCADE_LAYER)
        .min(UNLAYERED_CASCADE_LAYER);
    layer
        .saturating_mul(CASCADE_SPECIFICITY_STRIDE)
        .saturating_add(specificity.min(MAX_NATIVE_SELECTOR_SPECIFICITY))
}

fn resolve_white_space(
    candidates: [Option<CascadeValue<WhiteSpaceDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: WhiteSpaceValue,
) -> WhiteSpaceValue {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        WhiteSpaceDeclaration::Value(value) => Some(value),
        WhiteSpaceDeclaration::RevertLayer => None,
    })
}

fn resolve_line_height(
    candidates: [Option<CascadeValue<LineHeightDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: Option<u32>,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        LineHeightDeclaration::Value(value) => Some(Some(value)),
        LineHeightDeclaration::RevertLayer => None,
    })
}

fn resolve_direction(
    candidates: [Option<CascadeValue<DirectionDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: DirectionValue,
) -> DirectionValue {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        DirectionDeclaration::Value(value) => Some(value),
        DirectionDeclaration::RevertLayer => None,
    })
}

fn resolve_flex_direction(
    candidates: [Option<CascadeValue<FlexDirectionDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> FlexDirectionValue {
    resolve_alignment_candidates(candidates, FlexDirectionValue::Row, |declaration| {
        match declaration {
            FlexDirectionDeclaration::Value(value) => Some(value),
            FlexDirectionDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_justify_content(
    candidates: [Option<CascadeValue<JustifyContentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> JustifyContentValue {
    resolve_alignment_candidates(candidates, JustifyContentValue::FlexStart, |declaration| {
        match declaration {
            JustifyContentDeclaration::Value(value) => Some(value),
            JustifyContentDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_align_items(
    candidates: [Option<CascadeValue<AlignItemsDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> AlignItemsValue {
    resolve_alignment_candidates(candidates, AlignItemsValue::FlexStart, |declaration| {
        match declaration {
            AlignItemsDeclaration::Value(value) => Some(value),
            AlignItemsDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_align_self(
    candidates: [Option<CascadeValue<AlignSelfDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> AlignSelfValue {
    resolve_alignment_candidates(
        candidates,
        AlignSelfValue::Auto,
        |declaration| match declaration {
            AlignSelfDeclaration::Value(value) => Some(value),
            AlignSelfDeclaration::RevertLayer => None,
        },
    )
}

fn resolve_align_content(
    candidates: [Option<CascadeValue<AlignContentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> AlignContentValue {
    resolve_alignment_candidates(candidates, AlignContentValue::FlexStart, |declaration| {
        match declaration {
            AlignContentDeclaration::Value(value) => Some(value),
            AlignContentDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_flex_wrap(
    candidates: [Option<CascadeValue<FlexWrapDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> FlexWrapValue {
    resolve_alignment_candidates(
        candidates,
        FlexWrapValue::NoWrap,
        |declaration| match declaration {
            FlexWrapDeclaration::Value(value) => Some(value),
            FlexWrapDeclaration::RevertLayer => None,
        },
    )
}

fn resolve_flex_item_order(
    candidates: [Option<CascadeValue<FlexItemOrderDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> NativeOrderValue {
    resolve_alignment_candidates(candidates, NativeOrderValue::default(), |declaration| {
        match declaration {
            FlexItemOrderDeclaration::Value(value) => Some(value),
            FlexItemOrderDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_flex_grow(
    candidates: [Option<CascadeValue<FlexGrowDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> u32 {
    resolve_alignment_candidates(candidates, 0, |declaration| match declaration {
        FlexGrowDeclaration::Value(value) => Some(value),
        FlexGrowDeclaration::RevertLayer => None,
    })
}

fn resolve_flex_shrink(
    candidates: [Option<CascadeValue<FlexShrinkDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> u32 {
    resolve_alignment_candidates(candidates, 1, |declaration| match declaration {
        FlexShrinkDeclaration::Value(value) => Some(value),
        FlexShrinkDeclaration::RevertLayer => None,
    })
}

fn resolve_flex_basis(
    candidates: [Option<CascadeValue<FlexBasisDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> FlexBasisValue {
    resolve_alignment_candidates(
        candidates,
        FlexBasisValue::Auto,
        |declaration| match declaration {
            FlexBasisDeclaration::Value(value) => Some(value),
            FlexBasisDeclaration::RevertLayer => None,
        },
    )
}

fn resolve_text_align(
    candidates: [Option<CascadeValue<TextAlignDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: TextAlignValue,
) -> TextAlignValue {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        TextAlignDeclaration::Value(value) => Some(value),
        TextAlignDeclaration::RevertLayer => None,
    })
}

fn resolve_text_align_last(
    candidates: [Option<CascadeValue<TextAlignLastDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: TextAlignLastValue,
) -> TextAlignLastValue {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        TextAlignLastDeclaration::Value(value) => Some(value),
        TextAlignLastDeclaration::RevertLayer => None,
    })
}

fn resolve_text_justify(
    candidates: [Option<CascadeValue<TextJustifyDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: TextJustifyValue,
) -> TextJustifyValue {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        TextJustifyDeclaration::Value(value) => Some(value),
        TextJustifyDeclaration::RevertLayer => None,
    })
}

fn resolve_inherited_text_declaration<T: Copy>(
    candidates: [Option<CascadeValue<InheritedTextDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: T,
) -> T {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        InheritedTextDeclaration::Value(value) => Some(value),
        InheritedTextDeclaration::RevertLayer => None,
    })
}

fn resolve_local_cascade_declaration<T: Copy>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS],
    fallback: T,
) -> T {
    resolve_alignment_candidates(candidates, fallback, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(value),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_optional_cascade_declaration<T: Copy>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS],
) -> Option<T> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(value)),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_alignment_candidates<T: Copy, U: Copy>(
    candidates: [Option<CascadeValue<T>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: U,
    value: impl Fn(T) -> Option<U>,
) -> U {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if value(candidate.value).is_none() {
            blocked[layer] = true;
            continue;
        }
        return value(candidate.value).unwrap_or(inherited);
    }
}

fn resolve_text_decoration_skip_spaces(
    candidates: [Option<CascadeValue<NativeTextDecorationSkipSpacesDeclaration>>;
        MAX_NATIVE_CASCADE_LAYERS],
    inherited: NativeTextDecorationSkipSpaces,
) -> NativeTextDecorationSkipSpaces {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationSkipSpacesDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return candidate.value.resolve(inherited);
    }
}

fn resolve_text_decoration_style(
    candidates: [Option<CascadeValue<NativeTextDecorationStyleDeclaration>>;
        MAX_NATIVE_CASCADE_LAYERS],
    inherited: NativeTextDecorationStyle,
) -> NativeTextDecorationStyle {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationStyleDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return candidate.value.resolve(inherited);
    }
}

fn resolve_text_decoration_thickness(
    candidates: [Option<CascadeValue<NativeTextDecorationThicknessDeclaration>>;
        MAX_NATIVE_CASCADE_LAYERS],
    inherited: u32,
) -> u32 {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationThicknessDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return candidate.value.resolve(inherited);
    }
}

fn resolve_text_underline_offset(
    candidates: [Option<CascadeValue<NativeTextUnderlineOffsetDeclaration>>;
        MAX_NATIVE_CASCADE_LAYERS],
    inherited: i32,
) -> i32 {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextUnderlineOffsetDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return candidate.value.resolve(inherited);
    }
}

fn resolve_text_decoration(
    candidates: [Option<CascadeValue<NativeTextDecorationDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    inherited: TextDecorationValue,
) -> TextDecorationValue {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return candidate.value.resolve(inherited);
    }
}

fn resolve_text_decoration_color(
    candidates: [Option<CascadeValue<NativeTextDecorationColorDeclaration>>;
        MAX_NATIVE_CASCADE_LAYERS],
) -> Option<NativeColor> {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let (layer, candidate) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })?;
        if matches!(
            candidate.value,
            NativeTextDecorationColorDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return match candidate.value {
            NativeTextDecorationColorDeclaration::Value(value) => Some(value),
            NativeTextDecorationColorDeclaration::RevertLayer => unreachable!(),
        };
    }
}

fn resolve_text_decoration_skip_ink(
    candidates: [Option<CascadeValue<NativeTextDecorationSkipInkDeclaration>>;
        MAX_NATIVE_CASCADE_LAYERS],
    inherited: NativeTextDecorationSkipInk,
) -> NativeTextDecorationSkipInk {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            candidates
                .iter()
                .enumerate()
                .rev()
                .find_map(|(layer, candidate)| {
                    if blocked[layer] {
                        None
                    } else {
                        candidate.map(|candidate| (layer, candidate))
                    }
                })
        else {
            return inherited;
        };
        if matches!(
            candidate.value,
            NativeTextDecorationSkipInkDeclaration::RevertLayer
        ) {
            blocked[layer] = true;
            continue;
        }
        return candidate.value.resolve(inherited);
    }
}

fn gap_precedes<T, U>(candidate: &GapCascadeValue<T>, current: &GapCascadeValue<U>) -> bool {
    (
        candidate.inline,
        candidate.specificity,
        candidate.order,
        candidate.declaration_order,
    ) > (
        current.inline,
        current.specificity,
        current.order,
        current.declaration_order,
    )
}

fn gap_wins<T>(
    specificity: u16,
    order: usize,
    declaration_order: usize,
    inline: bool,
    current: Option<GapCascadeValue<T>>,
) -> bool {
    current.is_none_or(|current| {
        (inline, specificity, order, declaration_order)
            > (
                current.inline,
                current.specificity,
                current.order,
                current.declaration_order,
            )
    })
}

fn set_gap_candidate<T: Copy>(
    slot: &mut Option<GapCascadeValue<T>>,
    value: T,
    specificity: u16,
    order: usize,
    declaration_order: usize,
    inline: bool,
) {
    if gap_wins(specificity, order, declaration_order, inline, *slot) {
        *slot = Some(GapCascadeValue {
            value,
            specificity,
            order,
            declaration_order,
            inline,
        });
    }
}

fn select_gap_candidate(
    shorthand: Option<GapCascadeValue<GapComponentDeclaration>>,
    longhand: Option<GapCascadeValue<GapComponentDeclaration>>,
) -> Option<GapCascadeValue<GapComponentDeclaration>> {
    match (shorthand, longhand) {
        (Some(shorthand), Some(longhand)) if gap_precedes(&longhand, &shorthand) => Some(longhand),
        (Some(shorthand), _) => Some(shorthand),
        (None, Some(longhand)) => Some(longhand),
        (None, None) => None,
    }
}

fn resolve_gap_axis(
    shorthand: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
    longhand: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_CASCADE_LAYERS],
) -> u32 {
    let mut blocked = [false; MAX_NATIVE_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) = (0..MAX_NATIVE_CASCADE_LAYERS).rev().find_map(|layer| {
            if blocked[layer] {
                None
            } else {
                select_gap_candidate(shorthand[layer], longhand[layer])
                    .map(|candidate| (layer, candidate))
            }
        }) else {
            return 0;
        };
        match candidate.value {
            GapComponentDeclaration::Value(value) => return value,
            GapComponentDeclaration::RevertLayer => blocked[layer] = true,
        }
    }
}

fn apply_gap_declarations(
    declarations: NativeDeclarations,
    specificity: u16,
    order: usize,
    inline: bool,
    cascade: &mut GapCascade,
) {
    let layer = cascade_layer_index(specificity);
    if let Some(value) = declarations.gap {
        let (row, column) = match value {
            GapShorthandDeclaration::Value(value) => (
                GapComponentDeclaration::Value(value.row),
                GapComponentDeclaration::Value(value.column),
            ),
            GapShorthandDeclaration::RevertLayer => (
                GapComponentDeclaration::RevertLayer,
                GapComponentDeclaration::RevertLayer,
            ),
        };
        set_gap_candidate(
            &mut cascade.shorthand_row[layer],
            row,
            specificity,
            order,
            declarations.gap_order,
            inline,
        );
        set_gap_candidate(
            &mut cascade.shorthand_column[layer],
            column,
            specificity,
            order,
            declarations.gap_order,
            inline,
        );
    }
    if let Some(value) = declarations.row_gap {
        set_gap_candidate(
            &mut cascade.row_gap[layer],
            value,
            specificity,
            order,
            declarations.row_gap_order,
            inline,
        );
    }
    if let Some(value) = declarations.column_gap {
        set_gap_candidate(
            &mut cascade.column_gap[layer],
            value,
            specificity,
            order,
            declarations.column_gap_order,
            inline,
        );
    }
}

fn apply_inherited_text_declaration<T: Copy>(
    declaration: Option<InheritedTextDeclaration<T>>,
    specificity: u16,
    order: usize,
    inline: bool,
    candidates: &mut [Option<CascadeValue<InheritedTextDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = cascade_layer_index(specificity);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

fn apply_local_cascade_declaration<T: Copy>(
    declaration: Option<LocalCascadeDeclaration<T>>,
    specificity: u16,
    order: usize,
    inline: bool,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = cascade_layer_index(specificity);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

fn border_cascade_order(rule_order: usize, declaration_order: usize, inline: bool) -> usize {
    if inline {
        declaration_order
    } else {
        rule_order
            .saturating_mul(CASCADE_DECLARATION_ORDER_STRIDE)
            .saturating_add(declaration_order)
    }
}

fn apply_border_width_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<u32>>>; MAX_NATIVE_CASCADE_LAYERS];
             4],
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_width = declarations.border[index].and_then(|declaration| match declaration {
            LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => {
                Some(LocalCascadeDeclaration::Value(border.width()))
            }
            LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::None | NativeBorderDeclaration::Hidden,
            ) => None,
            LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
        });
        apply_local_cascade_declaration(
            border_width,
            specificity,
            border_cascade_order(rule_order, declarations.border_order[index], inline),
            inline,
            candidate,
        );
        apply_local_cascade_declaration(
            declarations.border_width[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_width_order[index], inline),
            inline,
            candidate,
        );
    }
}

fn apply_border_style_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>>;
             MAX_NATIVE_CASCADE_LAYERS]; 4],
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_style = declarations.border[index].map(|declaration| match declaration {
            LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => {
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(border.style()))
            }
            LocalCascadeDeclaration::Value(NativeBorderDeclaration::None) => {
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::None)
            }
            LocalCascadeDeclaration::Value(NativeBorderDeclaration::Hidden) => {
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Hidden)
            }
            LocalCascadeDeclaration::RevertLayer => LocalCascadeDeclaration::RevertLayer,
        });
        apply_local_cascade_declaration(
            border_style,
            specificity,
            border_cascade_order(rule_order, declarations.border_order[index], inline),
            inline,
            candidate,
        );
        apply_local_cascade_declaration(
            declarations.border_style[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_style_order[index], inline),
            inline,
            candidate,
        );
    }
}

fn apply_border_color_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<NativeColor>>>; MAX_NATIVE_CASCADE_LAYERS];
             4],
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_order =
            border_cascade_order(rule_order, declarations.border_order[index], inline);
        let border_color = declarations.border[index].and_then(|declaration| match declaration {
            LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => {
                Some(LocalCascadeDeclaration::Value(border.color()))
            }
            LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::None | NativeBorderDeclaration::Hidden,
            ) => None,
            LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
        });
        apply_local_cascade_declaration(border_color, specificity, border_order, inline, candidate);
        apply_local_cascade_declaration(
            declarations.border_color[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_color_order[index], inline),
            inline,
            candidate,
        );
    }
}

fn apply_local_cascade_edges<T: Copy>(
    declarations: &[Option<LocalCascadeDeclaration<T>>; 4],
    specificity: u16,
    order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS];
             4],
) {
    for (index, declaration) in declarations.iter().copied().enumerate() {
        apply_local_cascade_declaration(
            declaration,
            specificity,
            order,
            inline,
            &mut candidates[index],
        );
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeDeclarations {
    display: Option<LocalCascadeDeclaration<DisplayValue>>,
    visibility: Option<LocalCascadeDeclaration<VisibilityValue>>,
    opacity: Option<LocalCascadeDeclaration<u8>>,
    white_space: Option<WhiteSpaceDeclaration>,
    text_align: Option<TextAlignDeclaration>,
    text_align_last: Option<TextAlignLastDeclaration>,
    text_justify: Option<TextJustifyDeclaration>,
    justify_content: Option<JustifyContentDeclaration>,
    align_items: Option<AlignItemsDeclaration>,
    align_self: Option<AlignSelfDeclaration>,
    align_content: Option<AlignContentDeclaration>,
    flex_direction: Option<FlexDirectionDeclaration>,
    direction: Option<DirectionDeclaration>,
    flex_wrap: Option<FlexWrapDeclaration>,
    order: Option<FlexItemOrderDeclaration>,
    flex_grow: Option<FlexGrowDeclaration>,
    flex_shrink: Option<FlexShrinkDeclaration>,
    flex_basis: Option<FlexBasisDeclaration>,
    text_decoration: Option<NativeTextDecorationDeclaration>,
    text_decoration_style: Option<NativeTextDecorationStyleDeclaration>,
    text_decoration_skip_ink: Option<NativeTextDecorationSkipInkDeclaration>,
    text_decoration_skip_spaces: Option<NativeTextDecorationSkipSpacesDeclaration>,
    text_decoration_thickness: Option<NativeTextDecorationThicknessDeclaration>,
    text_underline_offset: Option<NativeTextUnderlineOffsetDeclaration>,
    text_decoration_color: Option<NativeTextDecorationColorDeclaration>,
    text_transform: Option<InheritedTextDeclaration<TextTransformValue>>,
    font_weight: Option<InheritedTextDeclaration<FontWeightValue>>,
    font_style: Option<InheritedTextDeclaration<FontStyleValue>>,
    word_break: Option<InheritedTextDeclaration<WordBreakValue>>,
    text_overflow: Option<LocalCascadeDeclaration<TextOverflowValue>>,
    vertical_align: Option<InheritedTextDeclaration<VerticalAlignValue>>,
    text_indent: Option<LocalCascadeDeclaration<u32>>,
    word_spacing: Option<InheritedTextDeclaration<u32>>,
    letter_spacing: Option<InheritedTextDeclaration<u32>>,
    gap: Option<GapShorthandDeclaration>,
    gap_order: usize,
    row_gap: Option<GapComponentDeclaration>,
    row_gap_order: usize,
    column_gap: Option<GapComponentDeclaration>,
    column_gap_order: usize,
    width: Option<LocalCascadeDeclaration<u32>>,
    height: Option<LocalCascadeDeclaration<u32>>,
    min_width: Option<LocalCascadeDeclaration<u32>>,
    max_width: Option<LocalCascadeDeclaration<u32>>,
    min_height: Option<LocalCascadeDeclaration<u32>>,
    max_height: Option<LocalCascadeDeclaration<u32>>,
    line_height: Option<LineHeightDeclaration>,
    background_color: Option<LocalCascadeDeclaration<NativeColor>>,
    border: [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; 4],
    border_order: [usize; 4],
    border_width: [Option<LocalCascadeDeclaration<u32>>; 4],
    border_width_order: [usize; 4],
    border_style: [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; 4],
    border_style_order: [usize; 4],
    border_color: [Option<LocalCascadeDeclaration<NativeColor>>; 4],
    border_color_order: [usize; 4],
    border_radius: Option<LocalCascadeDeclaration<NativeBorderRadius>>,
    padding: [Option<LocalCascadeDeclaration<u32>>; 4],
    margin: [Option<LocalCascadeDeclaration<NativeMarginValue>>; 4],
    box_sizing: Option<LocalCascadeDeclaration<NativeBoxSizing>>,
    color: Option<LocalCascadeDeclaration<NativeColor>>,
    overflow: Option<LocalCascadeDeclaration<OverflowValue>>,
    overflow_x: Option<LocalCascadeDeclaration<OverflowValue>>,
    overflow_y: Option<LocalCascadeDeclaration<OverflowValue>>,
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
    layers: &mut Vec<String>,
) -> Result<(), NativeEngineError> {
    let source = strip_comments(source);
    let end = source.len();
    let mut context = NativeCssParseContext {
        source: &source,
        rules,
        next_order,
        diagnostic_source,
        diagnostics,
        layers,
    };
    parse_source_block(&mut context, 0, end, None)
}

struct NativeCssParseContext<'a> {
    source: &'a str,
    rules: &'a mut Vec<NativeStyleRule>,
    next_order: &'a mut usize,
    diagnostic_source: NativeDiagnosticSource,
    diagnostics: &'a mut NativeDiagnosticSink,
    layers: &'a mut Vec<String>,
}

fn parse_source_block(
    context: &mut NativeCssParseContext<'_>,
    start: usize,
    end: usize,
    current_layer: Option<usize>,
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
        if let Some(layer_header) = parse_layer_header(header) {
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
                        parse_source_block(context, open + 1, close, Some(layer))?;
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
            parse_style_rule(context, cursor, open, close, current_layer)?;
        }
        cursor = close + 1;
    }
    Ok(())
}

fn parse_style_rule(
    context: &mut NativeCssParseContext<'_>,
    selector_start: usize,
    open: usize,
    close: usize,
    layer: Option<usize>,
) -> Result<(), NativeEngineError> {
    let source = context.source;
    let declarations = parse_declarations_with_diagnostics(
        &source[open + 1..close],
        context.diagnostic_source,
        open.saturating_add(1),
        context.diagnostics,
    );
    let has_supported_declaration = declarations.display.is_some()
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
        || declarations.font_weight.is_some()
        || declarations.font_style.is_some()
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
        || declarations.border.iter().any(Option::is_some)
        || declarations.border_width.iter().any(Option::is_some)
        || declarations.border_style.iter().any(Option::is_some)
        || declarations.border_color.iter().any(Option::is_some)
        || declarations.border_radius.is_some()
        || declarations.padding.iter().any(Option::is_some)
        || declarations.margin.iter().any(Option::is_some)
        || declarations.box_sizing.is_some()
        || declarations.color.is_some()
        || declarations.overflow.is_some()
        || declarations.overflow_x.is_some()
        || declarations.overflow_y.is_some();
    let selector_source = &source[selector_start..open];
    let mut selector_offset = selector_start;
    for selector_text in selector_source.split(',') {
        let Some(mut selector) = parse_selector(selector_text) else {
            context.diagnostics.push(
                NativeDiagnosticCode::UnsupportedCssSelector,
                context.diagnostic_source,
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
                order: *context.next_order,
            });
            *context.next_order = context.next_order.saturating_add(1);
        }
        selector_offset = selector_offset.saturating_add(selector_text.len() + 1);
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
            "display" => supports_display_declaration(value),
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
            "font-weight" => parse_font_weight_declaration(value).is_some(),
            "font-style" => parse_font_style_declaration(value).is_some(),
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
            "background-color" | "color" => parse_local_color_declaration(value).is_some(),
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                parse_border_declaration(value).is_some()
            }
            "border-width" => parse_border_width_declaration(value).is_some(),
            "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width" => parse_border_width_side_declaration(value).is_some(),
            "border-style" => parse_border_style_declaration(value).is_some(),
            "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style" => parse_border_style_side_declaration(value).is_some(),
            "border-color" => parse_border_color_declaration(value).is_some(),
            "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color" => parse_border_color_side_declaration(value).is_some(),
            "border-radius" => parse_border_radius_declaration(value).is_some(),
            "padding" => parse_local_box_edges(value).is_some(),
            "margin" => parse_local_margin_edges(value).is_some(),
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                parse_local_padding_declaration(value).is_some()
            }
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
                parse_local_margin_declaration(value).is_some()
            }
            "box-sizing" => parse_local_box_sizing_declaration(value).is_some(),
            "overflow" | "overflow-x" | "overflow-y" => parse_overflow_declaration(value)
                .is_some_and(|value| {
                    matches!(
                        value,
                        LocalCascadeDeclaration::RevertLayer
                            | LocalCascadeDeclaration::Value(
                                OverflowValue::Hidden | OverflowValue::Clip
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

fn is_known_css_property(property: &str) -> bool {
    matches!(
        property,
        "display"
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
            | "font-weight"
            | "font-style"
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
            | "background-color"
            | "color"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "border-style"
            | "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style"
            | "border-color"
            | "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color"
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
    for (declaration_order, declaration) in source.split(';').enumerate() {
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
                if let Some(parsed) = parse_display_declaration(value) {
                    declarations.display = Some(parsed);
                }
            }
            "visibility" => {
                if let Some(parsed) = parse_visibility_declaration(value) {
                    declarations.visibility = Some(parsed);
                }
            }
            "opacity" => {
                if let Some(parsed) = parse_opacity_declaration(value) {
                    declarations.opacity = Some(parsed);
                }
            }
            "white-space" => {
                declarations.white_space = parse_white_space_declaration(value);
            }
            "text-align" => {
                declarations.text_align = parse_text_align_declaration(value);
            }
            "text-align-last" => {
                declarations.text_align_last = parse_text_align_last_declaration(value);
            }
            "text-justify" => {
                declarations.text_justify = parse_text_justify_declaration(value);
            }
            "justify-content" => {
                if let Some(parsed) = parse_justify_content_declaration(value) {
                    declarations.justify_content = Some(parsed);
                }
            }
            "place-content" => {
                if let Some((align_content, justify_content)) =
                    parse_place_content_declaration(value)
                {
                    declarations.align_content = Some(align_content);
                    declarations.justify_content = Some(justify_content);
                }
            }
            "align-items" => {
                declarations.align_items = parse_align_items_declaration(value);
            }
            "align-self" => {
                if let Some(parsed) = parse_align_self_declaration(value) {
                    declarations.align_self = Some(parsed);
                }
            }
            "align-content" => {
                if let Some(parsed) = parse_align_content_declaration(value) {
                    declarations.align_content = Some(parsed);
                }
            }
            "flex-direction" => {
                declarations.flex_direction = parse_flex_direction_declaration(value);
            }
            "direction" => {
                declarations.direction = parse_direction_declaration(value);
            }
            "flex-wrap" => {
                declarations.flex_wrap = parse_flex_wrap_declaration(value);
            }
            "flex-flow" => {
                if let Some((direction, wrap)) = parse_flex_flow_declaration(value) {
                    declarations.flex_direction = Some(direction);
                    declarations.flex_wrap = Some(wrap);
                }
            }
            "order" => {
                if let Some(parsed) = parse_flex_item_order_declaration(value) {
                    declarations.order = Some(parsed);
                }
            }
            "flex" => {
                if let Some((grow, shrink, basis)) = parse_flex_shorthand_declaration(value) {
                    declarations.flex_grow = Some(grow);
                    declarations.flex_shrink = Some(shrink);
                    declarations.flex_basis = Some(basis);
                }
            }
            "flex-grow" => {
                if let Some(parsed) = parse_flex_grow_declaration(value) {
                    declarations.flex_grow = Some(parsed);
                }
            }
            "flex-shrink" => {
                if let Some(parsed) = parse_flex_shrink_declaration(value) {
                    declarations.flex_shrink = Some(parsed);
                }
            }
            "flex-basis" => {
                if let Some(parsed) = parse_flex_basis_declaration(value) {
                    declarations.flex_basis = Some(parsed);
                }
            }
            "text-decoration" | "text-decoration-line" => {
                declarations.text_decoration = parse_text_decoration_declaration(value);
            }
            "text-decoration-style" => {
                declarations.text_decoration_style = parse_text_decoration_style(value);
            }
            "text-decoration-skip-ink" => {
                declarations.text_decoration_skip_ink = parse_text_decoration_skip_ink(value);
            }
            "text-decoration-skip-spaces" => {
                declarations.text_decoration_skip_spaces = parse_text_decoration_skip_spaces(value);
            }
            "text-decoration-thickness" => {
                declarations.text_decoration_thickness = parse_text_decoration_thickness(value);
            }
            "text-underline-offset" => {
                declarations.text_underline_offset = parse_text_underline_offset(value);
            }
            "text-decoration-color" => {
                declarations.text_decoration_color = parse_text_decoration_color(value);
            }
            "text-transform" => {
                if let Some(parsed) = parse_text_transform_declaration(value) {
                    declarations.text_transform = Some(parsed);
                }
            }
            "font-weight" => {
                if let Some(parsed) = parse_font_weight_declaration(value) {
                    declarations.font_weight = Some(parsed);
                }
            }
            "font-style" => {
                if let Some(parsed) = parse_font_style_declaration(value) {
                    declarations.font_style = Some(parsed);
                }
            }
            "word-break" => {
                if let Some(parsed) = parse_word_break_declaration(value) {
                    declarations.word_break = Some(parsed);
                }
            }
            "text-overflow" => {
                if let Some(parsed) = parse_text_overflow_declaration(value) {
                    declarations.text_overflow = Some(parsed);
                }
            }
            "vertical-align" => {
                if let Some(parsed) = parse_vertical_align_declaration(value) {
                    declarations.vertical_align = Some(parsed);
                }
            }
            "text-indent" => {
                if let Some(parsed) = parse_text_indent_declaration(value) {
                    declarations.text_indent = Some(parsed);
                }
            }
            "word-spacing" => {
                if let Some(parsed) = parse_word_spacing_declaration(value) {
                    declarations.word_spacing = Some(parsed);
                }
            }
            "letter-spacing" => {
                if let Some(parsed) = parse_letter_spacing_declaration(value) {
                    declarations.letter_spacing = Some(parsed);
                }
            }
            "gap" => {
                if let Some(parsed) = parse_gap_declaration(value) {
                    declarations.gap = Some(parsed);
                    declarations.gap_order = declaration_order;
                }
            }
            "row-gap" => {
                if let Some(parsed) = parse_gap_component_declaration(value) {
                    declarations.row_gap = Some(parsed);
                    declarations.row_gap_order = declaration_order;
                }
            }
            "column-gap" => {
                if let Some(parsed) = parse_gap_component_declaration(value) {
                    declarations.column_gap = Some(parsed);
                    declarations.column_gap_order = declaration_order;
                }
            }
            "width" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.width = Some(parsed);
                }
            }
            "height" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.height = Some(parsed);
                }
            }
            "min-width" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.min_width = Some(parsed);
                }
            }
            "max-width" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.max_width = Some(parsed);
                }
            }
            "min-height" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.min_height = Some(parsed);
                }
            }
            "max-height" => {
                if let Some(parsed) = parse_local_dimension_declaration(value) {
                    declarations.max_height = Some(parsed);
                }
            }
            "line-height" => {
                declarations.line_height = parse_line_height_declaration(value);
            }
            "background-color" => {
                if let Some(parsed) = parse_local_color_declaration(value) {
                    declarations.background_color = Some(parsed);
                }
            }
            "border" => {
                if let Some(border) = parse_border_declaration(value) {
                    declarations.border = [Some(border); 4];
                    declarations.border_order = [declaration_order; 4];
                }
            }
            "border-top" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    0,
                    value,
                    declaration_order,
                );
            }
            "border-right" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    1,
                    value,
                    declaration_order,
                );
            }
            "border-bottom" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    2,
                    value,
                    declaration_order,
                );
            }
            "border-left" => {
                set_border_side(
                    &mut declarations.border,
                    &mut declarations.border_order,
                    3,
                    value,
                    declaration_order,
                );
            }
            "border-width" => {
                if let Some(values) = parse_border_width_declaration(value) {
                    declarations.border_width = values.map(Some);
                    declarations.border_width_order = [declaration_order; 4];
                }
            }
            "border-top-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    0,
                    value,
                    declaration_order,
                );
            }
            "border-right-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    1,
                    value,
                    declaration_order,
                );
            }
            "border-bottom-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    2,
                    value,
                    declaration_order,
                );
            }
            "border-left-width" => {
                set_border_width_side(
                    &mut declarations.border_width,
                    &mut declarations.border_width_order,
                    3,
                    value,
                    declaration_order,
                );
            }
            "border-style" => {
                if let Some(values) = parse_border_style_declaration(value) {
                    declarations.border_style = values.map(Some);
                    declarations.border_style_order = [declaration_order; 4];
                }
            }
            "border-top-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    0,
                    value,
                    declaration_order,
                );
            }
            "border-right-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    1,
                    value,
                    declaration_order,
                );
            }
            "border-bottom-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    2,
                    value,
                    declaration_order,
                );
            }
            "border-left-style" => {
                set_border_style_side(
                    &mut declarations.border_style,
                    &mut declarations.border_style_order,
                    3,
                    value,
                    declaration_order,
                );
            }
            "border-color" => {
                if let Some(values) = parse_border_color_declaration(value) {
                    declarations.border_color = values.map(Some);
                    declarations.border_color_order = [declaration_order; 4];
                }
            }
            "border-top-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    0,
                    value,
                    declaration_order,
                );
            }
            "border-right-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    1,
                    value,
                    declaration_order,
                );
            }
            "border-bottom-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    2,
                    value,
                    declaration_order,
                );
            }
            "border-left-color" => {
                set_border_color_side(
                    &mut declarations.border_color,
                    &mut declarations.border_color_order,
                    3,
                    value,
                    declaration_order,
                );
            }
            "border-radius" => {
                if let Some(parsed) = parse_border_radius_declaration(value) {
                    declarations.border_radius = Some(parsed);
                }
            }
            "padding" => {
                if let Some(values) = parse_local_box_edges(value) {
                    declarations.padding = values.map(Some);
                }
            }
            "margin" => {
                if let Some(values) = parse_local_margin_edges(value) {
                    declarations.margin = values.map(Some);
                }
            }
            "padding-top" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[0] = Some(value);
                }
            }
            "padding-right" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[1] = Some(value);
                }
            }
            "padding-bottom" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[2] = Some(value);
                }
            }
            "padding-left" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[3] = Some(value);
                }
            }
            "margin-top" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[0] = Some(value);
                }
            }
            "margin-right" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[1] = Some(value);
                }
            }
            "margin-bottom" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[2] = Some(value);
                }
            }
            "margin-left" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[3] = Some(value);
                }
            }
            "box-sizing" => {
                if let Some(value) = parse_local_box_sizing_declaration(value) {
                    declarations.box_sizing = Some(value);
                }
            }
            "color" => {
                if let Some(parsed) = parse_local_color_declaration(value) {
                    declarations.color = Some(parsed);
                }
            }
            "overflow" => {
                if let Some(parsed) = parse_overflow_declaration(value) {
                    declarations.overflow = Some(parsed);
                    declarations.overflow_x = Some(parsed);
                    declarations.overflow_y = Some(parsed);
                }
            }
            "overflow-x" => {
                if let Some(parsed) = parse_overflow_declaration(value) {
                    declarations.overflow_x = Some(parsed);
                }
            }
            "overflow-y" => {
                if let Some(parsed) = parse_overflow_declaration(value) {
                    declarations.overflow_y = Some(parsed);
                }
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

fn parse_border_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderDeclaration>> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    if value.trim().eq_ignore_ascii_case("none") {
        return Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None,
        ));
    }
    if value.trim().eq_ignore_ascii_case("hidden") {
        return Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Hidden,
        ));
    }
    parse_border(value)
        .map(|border| LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)))
}

fn parse_border_width_declaration(value: &str) -> Option<[LocalCascadeDeclaration<u32>; 4]> {
    parse_local_box_edges(value)
}

fn parse_border_width_side_declaration(value: &str) -> Option<LocalCascadeDeclaration<u32>> {
    parse_local_dimension_declaration(value)
}

fn parse_border_style_declaration(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderStyleValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_style_value)
        .collect::<Option<Vec<_>>>()?;
    expand_box_edges(&values).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_border_style_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderStyleValue>> {
    parse_local_cascade_declaration(value, parse_border_style_value)
}

fn parse_border_color(value: &str) -> Option<[NativeColor; 4]> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_color)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some([*all; 4]),
        [top, right] => Some([*top, *right, *top, *right]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

fn parse_border_color_declaration(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeColor>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    parse_border_color(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_border_color_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeColor>> {
    parse_local_cascade_declaration(value, parse_color)
}

fn split_css_value_tokens(value: &str) -> Option<Vec<&str>> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut parentheses = 0usize;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => {
                if start.is_none() {
                    start = Some(index);
                }
                parentheses = parentheses.checked_add(1)?;
            }
            b')' => {
                parentheses = parentheses.checked_sub(1)?;
            }
            byte if byte.is_ascii_whitespace() && parentheses == 0 => {
                if let Some(start) = start.take() {
                    tokens.push(&value[start..index]);
                }
            }
            _ if start.is_none() => start = Some(index),
            _ => {}
        }
    }
    if parentheses != 0 {
        return None;
    }
    if let Some(start) = start {
        tokens.push(&value[start..]);
    }
    (!tokens.is_empty()).then_some(tokens)
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

fn parse_border_radius_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderRadius>> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    parse_border_radius(value).map(LocalCascadeDeclaration::Value)
}

fn parse_box_edges(value: &str) -> Option<[u32; 4]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    expand_box_edges(&values)
}

fn parse_local_box_edges(value: &str) -> Option<[LocalCascadeDeclaration<u32>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    parse_box_edges(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_margin_value(value: &str) -> Option<NativeMarginValue> {
    if value.eq_ignore_ascii_case("auto") {
        Some(NativeMarginValue::Auto)
    } else {
        parse_dimension(value).map(NativeMarginValue::Length)
    }
}

fn parse_margin_edges(value: &str) -> Option<[NativeMarginValue; 4]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_margin_value)
        .collect::<Option<Vec<_>>>()?;
    expand_box_edges(&values)
}

fn parse_local_margin_edges(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeMarginValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    parse_margin_edges(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn expand_box_edges<T: Copy>(values: &[T]) -> Option<[T; 4]> {
    match values {
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
        "double" => Some(NativeBorderStyle::Double),
        "groove" => Some(NativeBorderStyle::Groove),
        "ridge" => Some(NativeBorderStyle::Ridge),
        "inset" => Some(NativeBorderStyle::Inset),
        "outset" => Some(NativeBorderStyle::Outset),
        _ => None,
    }
}

fn parse_border_style_value(value: &str) -> Option<NativeBorderStyleValue> {
    if value.eq_ignore_ascii_case("none") {
        Some(NativeBorderStyleValue::None)
    } else if value.eq_ignore_ascii_case("hidden") {
        Some(NativeBorderStyleValue::Hidden)
    } else {
        parse_border_style(value).map(NativeBorderStyleValue::Paint)
    }
}

fn parse_text_decoration_style(value: &str) -> Option<NativeTextDecorationStyleDeclaration> {
    match value.to_ascii_lowercase().as_str() {
        "solid" => Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Solid,
        )),
        "dashed" => Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Dashed,
        )),
        "dotted" => Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Dotted,
        )),
        "double" => Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Double,
        )),
        "wavy" => Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Wavy,
        )),
        "revert-layer" => Some(NativeTextDecorationStyleDeclaration::RevertLayer),
        _ => None,
    }
}

fn parse_text_decoration_skip_ink(value: &str) -> Option<NativeTextDecorationSkipInkDeclaration> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(NativeTextDecorationSkipInkDeclaration::Value(
            NativeTextDecorationSkipInk::Auto,
        )),
        "none" => Some(NativeTextDecorationSkipInkDeclaration::Value(
            NativeTextDecorationSkipInk::None,
        )),
        "revert-layer" => Some(NativeTextDecorationSkipInkDeclaration::RevertLayer),
        _ => None,
    }
}

fn parse_text_decoration_skip_spaces(
    value: &str,
) -> Option<NativeTextDecorationSkipSpacesDeclaration> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
    match tokens.as_slice() {
        [token] => match token.to_ascii_lowercase().as_str() {
            "initial" => Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd,
            )),
            "inherit" => Some(NativeTextDecorationSkipSpacesDeclaration::Inherit),
            "unset" => Some(NativeTextDecorationSkipSpacesDeclaration::Unset),
            "revert" => Some(NativeTextDecorationSkipSpacesDeclaration::Revert),
            "revert-layer" => Some(NativeTextDecorationSkipSpacesDeclaration::RevertLayer),
            "none" => Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::None,
            )),
            "all" => Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::All,
            )),
            "start" => Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::Start,
            )),
            "end" => Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::End,
            )),
            _ => None,
        },
        [first, second] => {
            let first = first.to_ascii_lowercase();
            let second = second.to_ascii_lowercase();
            if matches!(first.as_str(), "start" | "end")
                && matches!(second.as_str(), "start" | "end")
                && first != second
            {
                Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                    NativeTextDecorationSkipSpaces::StartAndEnd,
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_text_decoration_thickness(
    value: &str,
) -> Option<NativeTextDecorationThicknessDeclaration> {
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationThicknessDeclaration::RevertLayer);
    }
    parse_dimension(value)
        .filter(|value| (1..=MAX_NATIVE_TEXT_DECORATION_THICKNESS).contains(value))
        .map(NativeTextDecorationThicknessDeclaration::Value)
}

fn parse_text_underline_offset(value: &str) -> Option<NativeTextUnderlineOffsetDeclaration> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextUnderlineOffsetDeclaration::RevertLayer);
    }
    let value = value.to_ascii_lowercase();
    let value = value.strip_suffix("px")?.trim();
    let (negative, magnitude) = match value.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, value),
    };
    if magnitude.is_empty() || !magnitude.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let magnitude = magnitude.parse::<i32>().ok()?;
    if magnitude > MAX_NATIVE_TEXT_UNDERLINE_OFFSET {
        return None;
    }
    Some(NativeTextUnderlineOffsetDeclaration::Value(if negative {
        -magnitude
    } else {
        magnitude
    }))
}

fn parse_text_decoration_color(value: &str) -> Option<NativeTextDecorationColorDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationColorDeclaration::RevertLayer);
    }
    parse_color(value).map(NativeTextDecorationColorDeclaration::Value)
}

fn parse_local_color_declaration(value: &str) -> Option<LocalCascadeDeclaration<NativeColor>> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    parse_color(value).map(LocalCascadeDeclaration::Value)
}

fn set_border_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; 4],
    orders: &mut [usize; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
) {
    if let Some(border) = parse_border_declaration(value) {
        sides[index] = Some(border);
        orders[index] = declaration_order;
    }
}

fn set_border_color_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeColor>>; 4],
    orders: &mut [usize; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
) {
    if let Some(color) = parse_border_color_side_declaration(value) {
        sides[index] = Some(color);
        orders[index] = declaration_order;
    }
}

fn set_border_width_side(
    sides: &mut [Option<LocalCascadeDeclaration<u32>>; 4],
    orders: &mut [usize; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
) {
    if let Some(width) = parse_border_width_side_declaration(value) {
        sides[index] = Some(width);
        orders[index] = declaration_order;
    }
}

fn set_border_style_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; 4],
    orders: &mut [usize; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
) {
    if let Some(style) = parse_border_style_side_declaration(value) {
        sides[index] = Some(style);
        orders[index] = declaration_order;
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

fn parse_display_declaration(value: &str) -> Option<LocalCascadeDeclaration<DisplayValue>> {
    parse_local_cascade_declaration(value, parse_display)
}

fn supports_display_declaration(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("revert-layer")
        || matches!(
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

fn parse_dimension(value: &str) -> Option<u32> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_suffix("px")?.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = value.parse::<u32>().ok()?;
    (value <= MAX_NATIVE_VIEWPORT_DIMENSION).then_some(value)
}

fn parse_gap(value: &str) -> Option<NativeGapValue> {
    let mut values = value.split_ascii_whitespace();
    let row = parse_dimension(values.next()?)?;
    let column = match values.next() {
        Some(value) => parse_dimension(value)?,
        None => row,
    };
    values
        .next()
        .is_none()
        .then_some(NativeGapValue { row, column })
}

fn parse_gap_declaration(value: &str) -> Option<GapShorthandDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(GapShorthandDeclaration::RevertLayer)
    } else {
        parse_gap(value).map(GapShorthandDeclaration::Value)
    }
}

fn parse_gap_component_declaration(value: &str) -> Option<GapComponentDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(GapComponentDeclaration::RevertLayer)
    } else {
        parse_dimension(value).map(GapComponentDeclaration::Value)
    }
}

fn parse_line_height(value: &str) -> Option<u32> {
    parse_dimension(value).filter(|value| *value > 0)
}

fn parse_line_height_declaration(value: &str) -> Option<LineHeightDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(LineHeightDeclaration::RevertLayer)
    } else {
        parse_line_height(value).map(LineHeightDeclaration::Value)
    }
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

fn parse_opacity_declaration(value: &str) -> Option<LocalCascadeDeclaration<u8>> {
    parse_local_cascade_declaration(value, parse_opacity)
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

fn parse_white_space_declaration(value: &str) -> Option<WhiteSpaceDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(WhiteSpaceDeclaration::RevertLayer)
    } else {
        parse_white_space(value).map(WhiteSpaceDeclaration::Value)
    }
}

fn parse_text_align(value: &str) -> Option<TextAlignValue> {
    match value.to_ascii_lowercase().as_str() {
        "left" => Some(TextAlignValue::Left),
        "center" => Some(TextAlignValue::Center),
        "right" => Some(TextAlignValue::Right),
        "start" => Some(TextAlignValue::Start),
        "end" => Some(TextAlignValue::End),
        "justify" => Some(TextAlignValue::Justify),
        _ => None,
    }
}

fn parse_text_align_declaration(value: &str) -> Option<TextAlignDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(TextAlignDeclaration::RevertLayer)
    } else {
        parse_text_align(value).map(TextAlignDeclaration::Value)
    }
}

fn parse_text_align_last(value: &str) -> Option<TextAlignLastValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(TextAlignLastValue::Auto),
        "left" => Some(TextAlignLastValue::Left),
        "center" => Some(TextAlignLastValue::Center),
        "right" => Some(TextAlignLastValue::Right),
        "start" => Some(TextAlignLastValue::Start),
        "end" => Some(TextAlignLastValue::End),
        "justify" => Some(TextAlignLastValue::Justify),
        _ => None,
    }
}

fn parse_text_align_last_declaration(value: &str) -> Option<TextAlignLastDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(TextAlignLastDeclaration::RevertLayer)
    } else {
        parse_text_align_last(value).map(TextAlignLastDeclaration::Value)
    }
}

fn parse_text_justify(value: &str) -> Option<TextJustifyValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(TextJustifyValue::Auto),
        "none" => Some(TextJustifyValue::None),
        "inter-word" => Some(TextJustifyValue::InterWord),
        _ => None,
    }
}

fn parse_text_justify_declaration(value: &str) -> Option<TextJustifyDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(TextJustifyDeclaration::RevertLayer)
    } else {
        parse_text_justify(value).map(TextJustifyDeclaration::Value)
    }
}

fn parse_justify_content(value: &str) -> Option<JustifyContentValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(JustifyContentValue::FlexStart),
        "normal" => Some(JustifyContentValue::Normal),
        "stretch" => Some(JustifyContentValue::Stretch),
        "center" => Some(JustifyContentValue::Center),
        "flex-end" => Some(JustifyContentValue::FlexEnd),
        "space-between" => Some(JustifyContentValue::SpaceBetween),
        "space-around" => Some(JustifyContentValue::SpaceAround),
        "space-evenly" => Some(JustifyContentValue::SpaceEvenly),
        _ => None,
    }
}

fn parse_justify_content_declaration(value: &str) -> Option<JustifyContentDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(JustifyContentDeclaration::RevertLayer)
    } else {
        parse_justify_content(value).map(JustifyContentDeclaration::Value)
    }
}

fn parse_place_content(value: &str) -> Option<(AlignContentValue, JustifyContentValue)> {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    match values.as_slice() {
        [shared] => {
            let justify_content = parse_justify_content(shared)?;
            let align_content = parse_align_content(shared)?;
            Some((align_content, justify_content))
        }
        [align_content, justify_content] => Some((
            parse_align_content(align_content)?,
            parse_justify_content(justify_content)?,
        )),
        _ => None,
    }
}

fn parse_place_content_declaration(
    value: &str,
) -> Option<(AlignContentDeclaration, JustifyContentDeclaration)> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some((
            AlignContentDeclaration::RevertLayer,
            JustifyContentDeclaration::RevertLayer,
        ));
    }
    let (align_content, justify_content) = parse_place_content(value)?;
    Some((
        AlignContentDeclaration::Value(align_content),
        JustifyContentDeclaration::Value(justify_content),
    ))
}

fn parse_align_items(value: &str) -> Option<AlignItemsValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(AlignItemsValue::FlexStart),
        "center" => Some(AlignItemsValue::Center),
        "flex-end" => Some(AlignItemsValue::FlexEnd),
        "stretch" => Some(AlignItemsValue::Stretch),
        "normal" => Some(AlignItemsValue::Normal),
        _ => None,
    }
}

fn parse_align_items_declaration(value: &str) -> Option<AlignItemsDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(AlignItemsDeclaration::RevertLayer)
    } else {
        parse_align_items(value).map(AlignItemsDeclaration::Value)
    }
}

fn parse_align_self(value: &str) -> Option<AlignSelfValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(AlignSelfValue::Auto),
        "flex-start" => Some(AlignSelfValue::FlexStart),
        "center" => Some(AlignSelfValue::Center),
        "flex-end" => Some(AlignSelfValue::FlexEnd),
        "stretch" => Some(AlignSelfValue::Stretch),
        "normal" => Some(AlignSelfValue::Normal),
        _ => None,
    }
}

fn parse_align_self_declaration(value: &str) -> Option<AlignSelfDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(AlignSelfDeclaration::RevertLayer)
    } else {
        parse_align_self(value).map(AlignSelfDeclaration::Value)
    }
}

fn parse_align_content(value: &str) -> Option<AlignContentValue> {
    match value.to_ascii_lowercase().as_str() {
        "flex-start" => Some(AlignContentValue::FlexStart),
        "center" => Some(AlignContentValue::Center),
        "flex-end" => Some(AlignContentValue::FlexEnd),
        "space-between" => Some(AlignContentValue::SpaceBetween),
        "space-around" => Some(AlignContentValue::SpaceAround),
        "space-evenly" => Some(AlignContentValue::SpaceEvenly),
        "stretch" => Some(AlignContentValue::Stretch),
        "normal" => Some(AlignContentValue::Normal),
        _ => None,
    }
}

fn parse_align_content_declaration(value: &str) -> Option<AlignContentDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(AlignContentDeclaration::RevertLayer)
    } else {
        parse_align_content(value).map(AlignContentDeclaration::Value)
    }
}

fn parse_flex_direction(value: &str) -> Option<FlexDirectionValue> {
    match value.to_ascii_lowercase().as_str() {
        "row" => Some(FlexDirectionValue::Row),
        "row-reverse" => Some(FlexDirectionValue::RowReverse),
        "column" => Some(FlexDirectionValue::Column),
        "column-reverse" => Some(FlexDirectionValue::ColumnReverse),
        _ => None,
    }
}

fn parse_flex_direction_declaration(value: &str) -> Option<FlexDirectionDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(FlexDirectionDeclaration::RevertLayer)
    } else {
        parse_flex_direction(value).map(FlexDirectionDeclaration::Value)
    }
}

fn parse_direction(value: &str) -> Option<DirectionValue> {
    match value.to_ascii_lowercase().as_str() {
        "ltr" => Some(DirectionValue::Ltr),
        "rtl" => Some(DirectionValue::Rtl),
        _ => None,
    }
}

fn parse_direction_declaration(value: &str) -> Option<DirectionDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(DirectionDeclaration::RevertLayer)
    } else {
        parse_direction(value).map(DirectionDeclaration::Value)
    }
}

fn parse_flex_wrap(value: &str) -> Option<FlexWrapValue> {
    match value.to_ascii_lowercase().as_str() {
        "nowrap" => Some(FlexWrapValue::NoWrap),
        "wrap" => Some(FlexWrapValue::Wrap),
        "wrap-reverse" => Some(FlexWrapValue::WrapReverse),
        _ => None,
    }
}

fn parse_flex_wrap_declaration(value: &str) -> Option<FlexWrapDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(FlexWrapDeclaration::RevertLayer)
    } else {
        parse_flex_wrap(value).map(FlexWrapDeclaration::Value)
    }
}

fn parse_flex_flow(value: &str) -> Option<(FlexDirectionValue, FlexWrapValue)> {
    let mut direction = None;
    let mut wrap = None;
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    if values.is_empty() || values.len() > 2 {
        return None;
    }
    for value in values {
        if let Some(parsed) = parse_flex_direction(value) {
            if direction.replace(parsed).is_some() {
                return None;
            }
        } else {
            let parsed = parse_flex_wrap(value)?;
            if wrap.replace(parsed).is_some() {
                return None;
            }
        }
    }
    Some((direction.unwrap_or_default(), wrap.unwrap_or_default()))
}

fn parse_flex_flow_declaration(
    value: &str,
) -> Option<(FlexDirectionDeclaration, FlexWrapDeclaration)> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some((
            FlexDirectionDeclaration::RevertLayer,
            FlexWrapDeclaration::RevertLayer,
        ));
    }
    let (direction, wrap) = parse_flex_flow(value)?;
    Some((
        FlexDirectionDeclaration::Value(direction),
        FlexWrapDeclaration::Value(wrap),
    ))
}

fn parse_flex_item_order(value: &str) -> Option<NativeOrderValue> {
    let value = value.trim();
    let digits = match value.as_bytes().first() {
        Some(b'+') | Some(b'-') => &value[1..],
        _ => value,
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<i32>().ok()?;
    (MIN_NATIVE_FLEX_ITEM_ORDER..=MAX_NATIVE_FLEX_ITEM_ORDER)
        .contains(&parsed)
        .then_some(NativeOrderValue(parsed))
}

fn parse_flex_item_order_declaration(value: &str) -> Option<FlexItemOrderDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(FlexItemOrderDeclaration::RevertLayer)
    } else {
        parse_flex_item_order(value).map(FlexItemOrderDeclaration::Value)
    }
}

fn parse_flex_grow(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    (0..=MAX_NATIVE_FLEX_GROW)
        .contains(&parsed)
        .then_some(parsed)
}

fn parse_flex_grow_declaration(value: &str) -> Option<FlexGrowDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(FlexGrowDeclaration::RevertLayer)
    } else {
        parse_flex_grow(value).map(FlexGrowDeclaration::Value)
    }
}

fn parse_flex_shrink(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    (0..=MAX_NATIVE_FLEX_SHRINK)
        .contains(&parsed)
        .then_some(parsed)
}

fn parse_flex_shrink_declaration(value: &str) -> Option<FlexShrinkDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(FlexShrinkDeclaration::RevertLayer)
    } else {
        parse_flex_shrink(value).map(FlexShrinkDeclaration::Value)
    }
}

fn parse_flex_basis(value: &str) -> Option<FlexBasisValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Some(FlexBasisValue::Auto);
    }
    parse_dimension(value).map(FlexBasisValue::Length)
}

fn parse_flex_basis_declaration(value: &str) -> Option<FlexBasisDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        Some(FlexBasisDeclaration::RevertLayer)
    } else {
        parse_flex_basis(value).map(FlexBasisDeclaration::Value)
    }
}

fn parse_flex_shorthand(value: &str) -> Option<(u32, u32, FlexBasisValue)> {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    match values.as_slice() {
        [value] if value.eq_ignore_ascii_case("none") => Some((0, 0, FlexBasisValue::Auto)),
        [value] if value.eq_ignore_ascii_case("auto") => Some((1, 1, FlexBasisValue::Auto)),
        [grow] => Some((parse_flex_grow(grow)?, 1, FlexBasisValue::Length(0))),
        [grow, second] => {
            let grow = parse_flex_grow(grow)?;
            if let Some(shrink) = parse_flex_shrink(second) {
                Some((grow, shrink, FlexBasisValue::Length(0)))
            } else {
                Some((grow, 1, parse_flex_basis(second)?))
            }
        }
        [grow, shrink, basis] => Some((
            parse_flex_grow(grow)?,
            parse_flex_shrink(shrink)?,
            parse_flex_basis(basis)?,
        )),
        _ => None,
    }
}

fn parse_flex_shorthand_declaration(
    value: &str,
) -> Option<(
    FlexGrowDeclaration,
    FlexShrinkDeclaration,
    FlexBasisDeclaration,
)> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some((
            FlexGrowDeclaration::RevertLayer,
            FlexShrinkDeclaration::RevertLayer,
            FlexBasisDeclaration::RevertLayer,
        ));
    }
    let (grow, shrink, basis) = parse_flex_shorthand(value)?;
    Some((
        FlexGrowDeclaration::Value(grow),
        FlexShrinkDeclaration::Value(shrink),
        FlexBasisDeclaration::Value(basis),
    ))
}

fn parse_text_decoration(value: &str) -> Option<TextDecorationValue> {
    let mut bits = 0;
    let mut token_count = 0;
    let mut saw_none = false;

    for token in value.split_ascii_whitespace() {
        token_count += 1;
        if token.eq_ignore_ascii_case("none") {
            if token_count != 1 {
                return None;
            }
            saw_none = true;
            continue;
        }
        if saw_none {
            return None;
        }
        let bit = if token.eq_ignore_ascii_case("underline") {
            TextDecorationValue::UNDERLINE
        } else if token.eq_ignore_ascii_case("overline") {
            TextDecorationValue::OVERLINE
        } else if token.eq_ignore_ascii_case("line-through") {
            TextDecorationValue::LINE_THROUGH
        } else {
            return None;
        };
        if bits & bit != 0 {
            return None;
        }
        bits |= bit;
    }

    if token_count == 0 {
        None
    } else if saw_none {
        Some(TextDecorationValue::none())
    } else {
        Some(TextDecorationValue::new(
            bits & TextDecorationValue::UNDERLINE != 0,
            bits & TextDecorationValue::OVERLINE != 0,
            bits & TextDecorationValue::LINE_THROUGH != 0,
        ))
    }
}

fn parse_text_decoration_declaration(value: &str) -> Option<NativeTextDecorationDeclaration> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationDeclaration::RevertLayer);
    }
    parse_text_decoration(value).map(NativeTextDecorationDeclaration::Value)
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

fn parse_inherited_text_declaration<T: Copy>(
    value: &str,
    parse: fn(&str) -> Option<T>,
) -> Option<InheritedTextDeclaration<T>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(InheritedTextDeclaration::RevertLayer);
    }
    parse(value).map(InheritedTextDeclaration::Value)
}

fn parse_local_cascade_declaration<T: Copy>(
    value: &str,
    parse: fn(&str) -> Option<T>,
) -> Option<LocalCascadeDeclaration<T>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    parse(value).map(LocalCascadeDeclaration::Value)
}

fn parse_text_transform_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextTransformValue>> {
    parse_inherited_text_declaration(value, parse_text_transform)
}

fn parse_font_weight_declaration(value: &str) -> Option<InheritedTextDeclaration<FontWeightValue>> {
    parse_inherited_text_declaration(value, parse_font_weight)
}

fn parse_font_style_declaration(value: &str) -> Option<InheritedTextDeclaration<FontStyleValue>> {
    parse_inherited_text_declaration(value, parse_font_style)
}

fn parse_word_break_declaration(value: &str) -> Option<InheritedTextDeclaration<WordBreakValue>> {
    parse_inherited_text_declaration(value, parse_word_break)
}

fn parse_word_spacing_declaration(value: &str) -> Option<InheritedTextDeclaration<u32>> {
    parse_inherited_text_declaration(value, parse_dimension)
}

fn parse_letter_spacing_declaration(value: &str) -> Option<InheritedTextDeclaration<u32>> {
    parse_inherited_text_declaration(value, parse_dimension)
}

fn parse_vertical_align_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<VerticalAlignValue>> {
    parse_inherited_text_declaration(value, parse_vertical_align)
}

fn parse_text_overflow_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<TextOverflowValue>> {
    parse_local_cascade_declaration(value, parse_text_overflow)
}

fn parse_text_indent_declaration(value: &str) -> Option<LocalCascadeDeclaration<u32>> {
    parse_local_cascade_declaration(value, parse_dimension)
}

fn parse_local_dimension_declaration(value: &str) -> Option<LocalCascadeDeclaration<u32>> {
    parse_local_cascade_declaration(value, parse_dimension)
}

fn parse_local_padding_declaration(value: &str) -> Option<LocalCascadeDeclaration<u32>> {
    parse_local_cascade_declaration(value, parse_dimension)
}

fn parse_local_margin_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeMarginValue>> {
    parse_local_cascade_declaration(value, parse_margin_value)
}

fn parse_local_box_sizing_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBoxSizing>> {
    parse_local_cascade_declaration(value, parse_box_sizing)
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

fn parse_visibility_declaration(value: &str) -> Option<LocalCascadeDeclaration<VisibilityValue>> {
    parse_local_cascade_declaration(value, parse_visibility)
}

fn parse_overflow(value: &str) -> Option<OverflowValue> {
    match value.to_ascii_lowercase().as_str() {
        "hidden" => Some(OverflowValue::Hidden),
        "clip" => Some(OverflowValue::Clip),
        "visible" | "auto" | "scroll" => Some(OverflowValue::Other),
        _ => None,
    }
}

fn parse_overflow_declaration(value: &str) -> Option<LocalCascadeDeclaration<OverflowValue>> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    parse_overflow(value).map(LocalCascadeDeclaration::Value)
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
    let specificity = compounds.iter().try_fold(0u32, |specificity, compound| {
        specificity.checked_add(u32::from(compound.specificity))
    })?;
    let specificity = u16::try_from(specificity).ok()?;
    if specificity > MAX_NATIVE_SELECTOR_SPECIFICITY {
        return None;
    }
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

    fn uniform_styled_border(
        width: u32,
        style: NativeBorderStyle,
        color: NativeColor,
    ) -> NativeBorder {
        let side = styled_border_side(width, style, color);
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
            "color: red; display: none !important; visibility: visible; opacity: 50%; white-space: pre-line; text-align: center; text-align-last: end; text-justify: inter-word; justify-content: space-between; align-items: flex-end; align-self: center; align-content: stretch; flex-direction: row-reverse; direction: RTL; flex-wrap: wrap-reverse; order: -12; flex-grow: 2; flex-shrink: 3; flex-basis: 40px; text-decoration: underline; text-decoration-style: dotted; text-decoration-thickness: 2px; text-underline-offset: -2px; text-indent: 12px; word-spacing: 12px; letter-spacing: 12px; gap: 12px 14px; row-gap: 13px; column-gap: 15px; font-weight: bold; font-style: italic; word-break: break-all; text-overflow: ellipsis; vertical-align: bottom; width: 240px; height: 30px; min-width: 12px; max-width: 400px; min-height: 14px; max-height: 500px; line-height: 28px; border: 2px solid #102030; border-radius: 1px 2px 3px 4px; padding: 4px; margin: 3px; box-sizing: border-box; overflow: hidden",
        );
        assert_eq!(
            declarations.display,
            Some(LocalCascadeDeclaration::Value(DisplayValue::None))
        );
        assert_eq!(
            declarations.visibility,
            Some(LocalCascadeDeclaration::Value(VisibilityValue::Other))
        );
        assert_eq!(
            declarations.opacity,
            Some(LocalCascadeDeclaration::Value(128))
        );
        assert_eq!(
            declarations.white_space,
            Some(WhiteSpaceDeclaration::Value(WhiteSpaceValue::PreLine))
        );
        assert_eq!(
            declarations.text_align,
            Some(TextAlignDeclaration::Value(TextAlignValue::Center))
        );
        assert_eq!(
            declarations.text_align_last,
            Some(TextAlignLastDeclaration::Value(TextAlignLastValue::End))
        );
        assert_eq!(
            declarations.text_justify,
            Some(TextJustifyDeclaration::Value(TextJustifyValue::InterWord))
        );
        assert_eq!(
            declarations.justify_content,
            Some(JustifyContentDeclaration::Value(
                JustifyContentValue::SpaceBetween
            ))
        );
        assert_eq!(
            declarations.align_items,
            Some(AlignItemsDeclaration::Value(AlignItemsValue::FlexEnd))
        );
        assert_eq!(
            declarations.align_self,
            Some(AlignSelfDeclaration::Value(AlignSelfValue::Center))
        );
        assert_eq!(
            declarations.align_content,
            Some(AlignContentDeclaration::Value(AlignContentValue::Stretch))
        );
        assert_eq!(
            declarations.flex_direction,
            Some(FlexDirectionDeclaration::Value(
                FlexDirectionValue::RowReverse
            ))
        );
        assert_eq!(
            declarations.direction,
            Some(DirectionDeclaration::Value(DirectionValue::Rtl))
        );
        assert_eq!(
            declarations.flex_wrap,
            Some(FlexWrapDeclaration::Value(FlexWrapValue::WrapReverse))
        );
        assert_eq!(
            declarations.order,
            Some(FlexItemOrderDeclaration::Value(NativeOrderValue(-12)))
        );
        assert_eq!(declarations.flex_grow, Some(FlexGrowDeclaration::Value(2)));
        assert_eq!(
            declarations.flex_shrink,
            Some(FlexShrinkDeclaration::Value(3))
        );
        assert_eq!(
            declarations.flex_basis,
            Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(40)))
        );
        assert_eq!(
            declarations.text_decoration,
            Some(NativeTextDecorationDeclaration::Value(
                TextDecorationValue::new(true, false, false)
            ))
        );
        assert_eq!(
            declarations.text_decoration_style,
            Some(NativeTextDecorationStyleDeclaration::Value(
                NativeTextDecorationStyle::Dotted
            ))
        );
        assert_eq!(
            declarations.text_decoration_thickness,
            Some(NativeTextDecorationThicknessDeclaration::Value(2))
        );
        assert_eq!(
            declarations.text_underline_offset,
            Some(NativeTextUnderlineOffsetDeclaration::Value(-2))
        );
        assert_eq!(
            declarations.text_indent,
            Some(LocalCascadeDeclaration::Value(12))
        );
        assert_eq!(
            declarations.word_spacing,
            Some(InheritedTextDeclaration::Value(12))
        );
        assert_eq!(
            declarations.letter_spacing,
            Some(InheritedTextDeclaration::Value(12))
        );
        assert_eq!(
            declarations.gap,
            Some(GapShorthandDeclaration::Value(NativeGapValue {
                row: 12,
                column: 14,
            }))
        );
        assert_eq!(
            declarations.row_gap,
            Some(GapComponentDeclaration::Value(13))
        );
        assert_eq!(
            declarations.column_gap,
            Some(GapComponentDeclaration::Value(15))
        );
        assert_eq!(
            declarations.font_weight,
            Some(InheritedTextDeclaration::Value(FontWeightValue::Bold))
        );
        assert_eq!(
            declarations.font_style,
            Some(InheritedTextDeclaration::Value(FontStyleValue::Italic))
        );
        assert_eq!(
            declarations.word_break,
            Some(InheritedTextDeclaration::Value(WordBreakValue::BreakAll))
        );
        assert_eq!(
            declarations.text_overflow,
            Some(LocalCascadeDeclaration::Value(TextOverflowValue::Ellipsis))
        );
        assert_eq!(
            declarations.vertical_align,
            Some(InheritedTextDeclaration::Value(VerticalAlignValue::Bottom))
        );
        assert_eq!(
            declarations.width,
            Some(LocalCascadeDeclaration::Value(240))
        );
        assert_eq!(
            declarations.height,
            Some(LocalCascadeDeclaration::Value(30))
        );
        assert_eq!(
            declarations.min_width,
            Some(LocalCascadeDeclaration::Value(12))
        );
        assert_eq!(
            declarations.max_width,
            Some(LocalCascadeDeclaration::Value(400))
        );
        assert_eq!(
            declarations.min_height,
            Some(LocalCascadeDeclaration::Value(14))
        );
        assert_eq!(
            declarations.max_height,
            Some(LocalCascadeDeclaration::Value(500))
        );
        assert_eq!(
            declarations.line_height,
            Some(LineHeightDeclaration::Value(28))
        );
        assert_eq!(
            declarations.color,
            Some(LocalCascadeDeclaration::Value(NativeColor::RED))
        );
        let parsed_color = NativeColor {
            red: 16,
            green: 32,
            blue: 48,
            alpha: 255,
        };
        assert_eq!(
            declarations.border,
            [Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(border_side(2, parsed_color)),
            )); 4]
        );
        assert_eq!(
            declarations.border_radius,
            Some(LocalCascadeDeclaration::Value(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 4,
            }))
        );
        assert_eq!(
            declarations.padding,
            [Some(LocalCascadeDeclaration::Value(4)); 4]
        );
        assert_eq!(
            declarations.margin,
            [Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(3))); 4]
        );
        assert_eq!(
            declarations.box_sizing,
            Some(LocalCascadeDeclaration::Value(NativeBoxSizing::BorderBox))
        );
        assert_eq!(
            declarations.overflow,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Hidden))
        );
        assert_eq!(
            declarations.overflow_x,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Hidden))
        );
        assert_eq!(
            declarations.overflow_y,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Hidden))
        );
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
            "border-top: 1px wavy red; border-right: -1px solid blue; border-bottom: 20000px solid red; border-left: 1em solid green",
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
    fn local_paint_color_parser_accepts_revert_layer_and_preserves_valid_values() {
        let blue = NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        };
        assert_eq!(
            parse_local_color_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_local_color_declaration("rgba(1, 2, 3, 0.5)"),
            Some(LocalCascadeDeclaration::Value(NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 128,
            }))
        );
        for value in [
            "revert-layer red",
            "red revert-layer",
            "inherit",
            "unset",
            "revert",
            "currentColor",
            "linear-gradient(red, blue)",
            "color(display-p3 1 0 0)",
        ] {
            assert_eq!(parse_local_color_declaration(value), None, "value={value}");
        }

        let declarations = parse_declarations(
            "background-color: red; background-color: currentColor; color: blue; color: inherit",
        );
        assert_eq!(
            declarations.background_color,
            Some(LocalCascadeDeclaration::Value(NativeColor::RED))
        );
        assert_eq!(
            declarations.color,
            Some(LocalCascadeDeclaration::Value(blue))
        );
        assert_eq!(
            parse_declarations("background-color: revert-layer").background_color,
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_declarations("color: REVERT-LAYER").color,
            Some(LocalCascadeDeclaration::RevertLayer)
        );
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
        assert_eq!(
            parse_border_radius_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_border_radius_declaration("1px 2px"),
            Some(LocalCascadeDeclaration::Value(NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 1,
                bottom_left: 2,
            }))
        );
        assert_eq!(parse_border_radius_declaration("revert"), None);
        assert_eq!(parse_border_radius_declaration("1px revert-layer"), None);
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
        assert_eq!(
            declarations.padding,
            [
                Some(LocalCascadeDeclaration::Value(1)),
                Some(LocalCascadeDeclaration::Value(2)),
                Some(LocalCascadeDeclaration::Value(3)),
                Some(LocalCascadeDeclaration::Value(5)),
            ]
        );
        assert_eq!(
            declarations.margin,
            [
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(6))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(7))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(8))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(7))),
            ]
        );
        let declarations = parse_declarations("margin: 6px 7px; margin-bottom: 8px");
        assert_eq!(
            declarations.margin,
            [
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(6))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(7))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(8))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(7))),
            ]
        );
        let declarations =
            parse_declarations("padding: 4px; padding-left: 50%; margin: 2px; margin-top: -1px");
        assert_eq!(
            declarations.padding,
            [Some(LocalCascadeDeclaration::Value(4)); 4]
        );
        assert_eq!(
            declarations.margin,
            [Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(2))); 4]
        );
    }

    #[test]
    fn margin_parser_preserves_auto_edges_and_rejects_unbounded_values() {
        assert_eq!(parse_margin_value("auto"), Some(NativeMarginValue::Auto));
        assert_eq!(parse_margin_value("AUTO"), Some(NativeMarginValue::Auto));
        assert_eq!(
            parse_margin_value("4px"),
            Some(NativeMarginValue::Length(4))
        );
        assert_eq!(parse_margin_value("-1px"), None);
        assert_eq!(parse_margin_value("50%"), None);
        assert_eq!(
            parse_margin_edges("auto 2px 3px 4px"),
            Some([
                NativeMarginValue::Auto,
                NativeMarginValue::Length(2),
                NativeMarginValue::Length(3),
                NativeMarginValue::Length(4),
            ])
        );
        assert_eq!(parse_margin_edges("auto -1px"), None);
        assert_eq!(parse_margin_edges("auto 2px 3px 4px 5px"), None);

        let declarations = parse_declarations(
            "margin: auto 2px; margin-left: 4px; margin-bottom: auto; margin-right: -1px",
        );
        assert_eq!(
            declarations.margin,
            [
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Auto)),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(2))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Auto)),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(4))),
            ]
        );
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
    fn display_visibility_revert_layer_rolls_back_named_and_inline_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { display:none; visibility:hidden; } #repeat { display:block; visibility:hidden; } #contents { display:contents; visibility:visible; } #fallback { display:revert-layer; visibility:revert-layer; } #inline { display:block; visibility:hidden; } } @layer theme { #named { display:block; visibility:visible; } #named { display:revert-layer; visibility:revert-layer; } #repeat { display:revert-layer; visibility:revert-layer; } #contents { display:block; visibility:hidden; } #contents { display:revert-layer; visibility:revert-layer; } } @layer top { #repeat { display:revert-layer; visibility:revert-layer; } } #named { display:revert-layer; visibility:revert-layer; } #repeat { display:revert-layer; visibility:revert-layer; } #contents { display:revert-layer; visibility:revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<button id='named'>Named</button>");
        let repeat = node("<button id='repeat'>Repeat</button>");
        let contents = node("<button id='contents'>Contents</button>");
        let fallback = node("<button id='fallback'>Fallback</button>");
        let inline = node(
            "<button id='inline' style='display:ReVeRt-LaYeR;visibility:revert-layer'>Inline</button>",
        );

        assert_eq!(
            stylesheet.computed_for(&named).display(),
            DisplayValue::None
        );
        assert!(stylesheet.computed_for(&named).hidden());
        assert_eq!(
            stylesheet.computed_for(&repeat).display(),
            DisplayValue::Block
        );
        assert!(stylesheet.computed_for(&repeat).hidden());
        assert_eq!(
            stylesheet.computed_for(&contents).display(),
            DisplayValue::Contents
        );
        assert!(!stylesheet.computed_for(&contents).hidden());
        assert_eq!(
            stylesheet.computed_for(&fallback).display(),
            DisplayValue::Auto
        );
        assert!(!stylesheet.computed_for(&fallback).hidden());
        assert_eq!(
            stylesheet.computed_for(&inline).display(),
            DisplayValue::Block
        );
        assert!(stylesheet.computed_for(&inline).hidden());

        let declarations = parse_declarations(
            "display: block; display: unsupported; visibility: hidden; visibility: unsupported",
        );
        assert_eq!(
            declarations.display,
            Some(LocalCascadeDeclaration::Value(DisplayValue::Block))
        );
        assert_eq!(
            declarations.visibility,
            Some(LocalCascadeDeclaration::Value(VisibilityValue::Hidden))
        );
        assert_eq!(
            parse_display_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_visibility_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#valid { display: ReVeRt-LaYeR; visibility: revert-layer; } #invalid { display: block inline; visibility: hidden visible; }"
                    .into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                        && diagnostic.detail == "display"
                })
                .count(),
            1
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                        && diagnostic.detail == "visibility"
                })
                .count(),
            1
        );
    }

    #[test]
    fn border_revert_layer_rolls_back_independent_physical_sides() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #card { border: 1px solid red; } #sides { border-top: 1px solid red; border-right: 2px solid green; border-bottom: 3px solid blue; border-left: 4px solid black; } #fallback { border: revert-layer; } #inline { border: 1px solid red; } } @layer theme { #card { border: 3px dashed blue; } #card { border: revert-layer; } #sides { border-top: 5px dotted green; border-right: 6px solid blue; border-bottom: 7px dashed black; border-left: 8px solid red; } #sides { border-top: revert-layer; border-bottom: revert-layer; } #inline { border: 3px solid green; } } #card { border: revert-layer; } #sides { border-top: revert-layer; border-right: revert-layer; border-left: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let card = node("<div id='card'>Card</div>");
        let sides = node("<div id='sides'>Sides</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node("<div id='inline' style='border:ReVeRt-LaYeR'>Inline</div>");

        let card_border = stylesheet.computed_for(&card).border.unwrap();
        assert_eq!(card_border.top(), border_side(1, NativeColor::RED));
        assert_eq!(card_border.right(), border_side(1, NativeColor::RED));
        assert_eq!(card_border.bottom(), border_side(1, NativeColor::RED));
        assert_eq!(card_border.left(), border_side(1, NativeColor::RED));

        let sides_border = stylesheet.computed_for(&sides).border.unwrap();
        assert_eq!(sides_border.top(), border_side(1, NativeColor::RED));
        assert_eq!(
            sides_border.right(),
            border_side(
                6,
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
            )
        );
        assert_eq!(
            sides_border.bottom(),
            border_side(
                3,
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
            )
        );
        assert_eq!(sides_border.left(), border_side(8, NativeColor::RED));
        assert_eq!(stylesheet.computed_for(&fallback).border, None);

        let inline_border = stylesheet.computed_for(&inline).border.unwrap();
        assert_eq!(
            inline_border.top(),
            border_side(
                3,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                },
            )
        );

        let declarations = parse_declarations(
            "border: 2px solid red; border: 1px wavy blue; border-top: ReVeRt-LaYeR; border-right: 3px dotted green; border-right: 1px dotted invalid",
        );
        assert_eq!(
            declarations.border,
            [
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::Complete(styled_border_side(
                        3,
                        NativeBorderStyle::Dotted,
                        NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    )),
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::Complete(border_side(2, NativeColor::RED)),
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::Complete(border_side(2, NativeColor::RED)),
                )),
            ]
        );
        assert_eq!(
            parse_border_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_border_declaration("NoNe"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::None
            ))
        );
        assert_eq!(
            parse_declarations("border: none; border-top: NONE; border-right: none; border-bottom: none; border-left: NoNe").border,
            [Some(LocalCascadeDeclaration::Value(NativeBorderDeclaration::None)); 4]
        );
        assert_eq!(
            parse_border_declaration("HiDdEn"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Hidden
            ))
        );
        assert_eq!(
            parse_declarations("border: hidden; border-top: HIDDEN; border-right: hidden; border-bottom: Hidden; border-left: hIdDeN").border,
            [Some(LocalCascadeDeclaration::Value(NativeBorderDeclaration::Hidden)); 4]
        );
        assert_eq!(parse_border_declaration("revert"), None);
        assert_eq!(parse_border_declaration("none solid"), None);
        assert_eq!(parse_border_declaration("hidden solid"), None);
        assert_eq!(parse_border_declaration("1px solid red dashed"), None);

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#valid { border: ReVeRt-LaYeR; border-top: 1px solid red; } #invalid { border-right: 1px wavy red; border-left: 1px solid; }"
                    .into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                        && diagnostic.detail == "border-right"
                })
                .count(),
            1
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                        && diagnostic.detail == "border-left"
                })
                .count(),
            1
        );
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border"
        }));
    }

    #[test]
    fn border_color_parser_expands_physical_values_and_rejects_mixed_forms() {
        let declarations = parse_declarations(
            "border-color: red green rgb(1, 2, 3) #123456; border-top-color: transparent; border-right-color: RGBA(4, 5, 6, 0.5); border-bottom-color: ReVeRt-LaYeR; border-left-color: #abcdef",
        );
        assert_eq!(
            declarations.border_color,
            [
                Some(LocalCascadeDeclaration::Value(NativeColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                })),
                Some(LocalCascadeDeclaration::Value(NativeColor {
                    red: 4,
                    green: 5,
                    blue: 6,
                    alpha: 128,
                })),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(NativeColor {
                    red: 171,
                    green: 205,
                    blue: 239,
                    alpha: 255,
                })),
            ]
        );
        assert_eq!(declarations.border_color_order, [1, 2, 3, 4]);
        assert_eq!(
            parse_border_color("red green blue black"),
            Some([
                NativeColor::RED,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                },
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
                NativeColor::BLACK,
            ])
        );
        assert_eq!(
            parse_border_color_declaration("revert-layer"),
            Some([LocalCascadeDeclaration::RevertLayer; 4])
        );
        for value in [
            "revert-layer red",
            "red revert-layer",
            "red green blue black white",
            "red currentColor",
            "rgb(1, 2, 3",
        ] {
            assert_eq!(parse_border_color_declaration(value), None, "{value}");
        }
        assert_eq!(
            parse_border_color_side_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(parse_border_color_side_declaration("red blue"), None);
    }

    #[test]
    fn border_color_revert_layer_resolves_component_candidates_in_order() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border: 2px solid red; border-color: red green blue black; } #repeat { border: 1px solid red; } #fallback { border: 1px solid red; } #same { border: 1px solid red; border-color: revert-layer; } #order-a { border-color: blue; border: 1px solid red; } #order-b { border: 1px solid red; border-color: blue; } } @layer theme { #named { border-color: blue green blue black; } #named { border-top-color: revert-layer; } #repeat { border-color: blue; } #repeat { border-color: revert-layer; } #fallback { border-color: revert-layer; } } @layer top { #repeat { border-color: revert-layer; } } #named { border-color: revert-layer; } #repeat { border-color: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let repeat = node("<div id='repeat'>Repeat</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let same = node("<div id='same'>Same</div>");
        let order_a = node("<div id='order-a'>Order A</div>");
        let order_b = node("<div id='order-b'>Order B</div>");

        let named_border = stylesheet.computed_for(&named).border.unwrap();
        assert_eq!(named_border.top(), border_side(2, NativeColor::RED));
        assert_eq!(
            named_border.right(),
            border_side(
                2,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                },
            )
        );
        assert_eq!(
            named_border.bottom(),
            border_side(
                2,
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
            )
        );
        assert_eq!(named_border.left(), border_side(2, NativeColor::BLACK));

        let repeat_border = stylesheet.computed_for(&repeat).border.unwrap();
        assert_eq!(repeat_border, uniform_border(1, NativeColor::RED));
        let fallback_border = stylesheet.computed_for(&fallback).border.unwrap();
        assert_eq!(fallback_border, uniform_border(1, NativeColor::RED));
        assert_eq!(
            stylesheet.computed_for(&same).border.unwrap(),
            uniform_border(1, NativeColor::BLACK)
        );
        assert_eq!(
            stylesheet.computed_for(&order_a).border.unwrap(),
            uniform_border(1, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&order_b).border.unwrap(),
            uniform_border(
                1,
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
            )
        );
    }

    #[test]
    fn border_width_parser_expands_physical_values_and_rejects_mixed_forms() {
        let declarations = parse_declarations(
            "border-width: 1px 2px 3px 4px; border-top-width: 5px; border-right-width: ReVeRt-LaYeR; border-bottom-width: 6px; border-left-width: 7px; border-left-width: invalid",
        );
        assert_eq!(
            declarations.border_width,
            [
                Some(LocalCascadeDeclaration::Value(5)),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(6)),
                Some(LocalCascadeDeclaration::Value(7)),
            ]
        );
        assert_eq!(declarations.border_width_order, [1, 2, 3, 4]);
        assert_eq!(
            parse_border_width_declaration("1px 2px 3px 4px"),
            Some([
                LocalCascadeDeclaration::Value(1),
                LocalCascadeDeclaration::Value(2),
                LocalCascadeDeclaration::Value(3),
                LocalCascadeDeclaration::Value(4),
            ])
        );
        assert_eq!(
            parse_border_width_declaration("revert-layer"),
            Some([LocalCascadeDeclaration::RevertLayer; 4])
        );
        for value in [
            "revert-layer 1px",
            "1px revert-layer",
            "1px 2px 3px 4px 5px",
            "1px -1px",
            "1.5px",
            "50%",
            "medium",
            "1px solid red",
        ] {
            assert_eq!(parse_border_width_declaration(value), None, "{value}");
        }
        assert_eq!(
            parse_border_width_side_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(parse_border_width_side_declaration("1px 2px"), None);
    }

    #[test]
    fn border_width_revert_layer_resolves_component_candidates_in_order() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border: 1px solid red; border-width: 2px 3px; } #fallback { border: 1px solid red; } #same { border: 1px solid red; border-width: revert-layer; } #order-a { border-width: 3px; border: 1px solid red; } #order-b { border: 1px solid red; border-width: 3px; } #width-only { border-width: 4px; } #inline { border: 1px solid red; } } @layer theme { #named { border-width: 5px 6px 7px 8px; } #named { border-top-width: revert-layer; } #fallback { border-width: 4px; } #fallback { border-width: revert-layer; } } @layer top { #fallback { border-width: revert-layer; } } #named { border-width: revert-layer; } #fallback { border-width: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let same = node("<div id='same'>Same</div>");
        let order_a = node("<div id='order-a'>Order A</div>");
        let order_b = node("<div id='order-b'>Order B</div>");
        let width_only = node("<div id='width-only'>Width only</div>");
        let inline =
            node("<div id='inline' style='border:1px solid red;border-width:4px'>Inline</div>");

        let named_border = stylesheet.computed_for(&named).border.unwrap();
        assert_eq!(named_border.top(), border_side(2, NativeColor::RED));
        assert_eq!(named_border.right(), border_side(6, NativeColor::RED));
        assert_eq!(named_border.bottom(), border_side(7, NativeColor::RED));
        assert_eq!(named_border.left(), border_side(8, NativeColor::RED));
        assert_eq!(
            stylesheet.computed_for(&fallback).border.unwrap(),
            uniform_border(1, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&same).border.unwrap(),
            uniform_border(0, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&order_a).border.unwrap(),
            uniform_border(1, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&order_b).border.unwrap(),
            uniform_border(3, NativeColor::RED)
        );
        assert_eq!(stylesheet.computed_for(&width_only).border, None);
        assert_eq!(
            stylesheet.computed_for(&inline).border.unwrap(),
            uniform_border(4, NativeColor::RED)
        );
    }

    #[test]
    fn border_style_parser_expands_physical_values_and_rejects_mixed_forms() {
        let declarations = parse_declarations(
            "border-style: solid dashed dotted solid; border-top-style: dotted; border-right-style: ReVeRt-LaYeR; border-bottom-style: invalid; border-left-style: dashed; border-left-style: invalid",
        );
        assert_eq!(
            declarations.border_style,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted,)
                )),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted,)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed,)
                )),
            ]
        );
        assert_eq!(declarations.border_style_order, [1, 2, 0, 4]);
        assert_eq!(
            parse_border_style_declaration("solid dashed dotted wavy"),
            None
        );
        assert_eq!(
            parse_border_style_declaration("solid dashed dotted solid"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dashed,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dotted,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid,
                )),
            ])
        );
        assert_eq!(
            parse_border_style_declaration("revert-layer"),
            Some([LocalCascadeDeclaration::RevertLayer; 4])
        );
        for value in [
            "revert-layer solid",
            "solid revert-layer",
            "solid dashed dotted solid double",
            "solid 1px",
        ] {
            assert_eq!(parse_border_style_declaration(value), None, "{value}");
        }
        assert_eq!(
            parse_border_style_side_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_border_style_declaration("none solid dashed none"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::None),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dashed,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::None),
            ])
        );
        assert_eq!(
            parse_border_style_declaration("hidden solid dashed hidden"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Hidden),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dashed,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Hidden),
            ])
        );
        assert_eq!(parse_border_style_side_declaration("solid dashed"), None);
        assert_eq!(
            parse_border_style_side_declaration("none"),
            Some(LocalCascadeDeclaration::Value(NativeBorderStyleValue::None))
        );
        assert_eq!(
            parse_border_style_side_declaration("HiDdEn"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderStyleValue::Hidden
            ))
        );
        assert_eq!(
            parse_border_style_declaration("double groove ridge inset"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Double,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Groove,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Ridge,
                )),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Inset,
                )),
            ])
        );
        assert_eq!(
            parse_border_style_side_declaration("OuTsEt"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Outset)
            ))
        );
    }

    #[test]
    fn border_style_revert_layer_resolves_component_candidates_in_order() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border: 1px solid red; border-style: dashed dotted; } #fallback { border: 1px solid red; } #same { border: 1px solid red; border-style: revert-layer; } #order-a { border-style: dotted; border: 1px solid red; } #order-b { border: 1px solid red; border-style: dashed; } #combined { border-width: 4px; border-style: dashed; border-color: green; } #style-only { border-style: dashed; } #inline { border: 1px solid red; } } @layer theme { #named { border-style: dotted solid dashed solid; } #named { border-top-style: revert-layer; } #fallback { border-style: dashed; } #fallback { border-style: revert-layer; } } @layer top { #fallback { border-style: revert-layer; } } #named { border-style: revert-layer; } #fallback { border-style: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let same = node("<div id='same'>Same</div>");
        let order_a = node("<div id='order-a'>Order A</div>");
        let order_b = node("<div id='order-b'>Order B</div>");
        let combined = node("<div id='combined'>Combined</div>");
        let style_only = node("<div id='style-only'>Style only</div>");
        let inline = node("<div id='inline' style='border-style:dotted'>Inline</div>");

        let named_border = stylesheet.computed_for(&named).border.unwrap();
        assert_eq!(
            named_border.top(),
            styled_border_side(1, NativeBorderStyle::Dashed, NativeColor::RED)
        );
        assert_eq!(
            named_border.right(),
            styled_border_side(1, NativeBorderStyle::Solid, NativeColor::RED)
        );
        assert_eq!(
            named_border.bottom(),
            styled_border_side(1, NativeBorderStyle::Dashed, NativeColor::RED)
        );
        assert_eq!(
            named_border.left(),
            styled_border_side(1, NativeBorderStyle::Solid, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&fallback).border.unwrap(),
            uniform_border(1, NativeColor::RED)
        );
        assert_eq!(stylesheet.computed_for(&same).border, None);
        assert_eq!(
            stylesheet.computed_for(&order_a).border.unwrap(),
            uniform_border(1, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&order_b).border.unwrap(),
            uniform_styled_border(1, NativeBorderStyle::Dashed, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&combined).border.unwrap(),
            NativeBorder {
                top: styled_border_side(
                    4,
                    NativeBorderStyle::Dashed,
                    NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 255,
                    }
                ),
                right: styled_border_side(
                    4,
                    NativeBorderStyle::Dashed,
                    NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 255,
                    }
                ),
                bottom: styled_border_side(
                    4,
                    NativeBorderStyle::Dashed,
                    NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 255,
                    }
                ),
                left: styled_border_side(
                    4,
                    NativeBorderStyle::Dashed,
                    NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 255,
                    }
                ),
            }
        );
        assert_eq!(stylesheet.computed_for(&style_only).border, None);
        assert_eq!(
            stylesheet.computed_for(&inline).border.unwrap(),
            uniform_styled_border(1, NativeBorderStyle::Dotted, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&inline).border.unwrap().top().style,
            NativeBorderStyle::Dotted
        );
    }

    #[test]
    fn border_style_none_blocks_or_reveals_lower_components_without_public_leakage() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #blocked { border: 2px solid red; } #exposed { border: 2px solid red; } #repeated { border: 2px solid red; } #invalid { border: 2px solid red; } #physical { border: 2px solid red; } #style-only { border-style: none; } #inline { border: 2px solid red; } #inline-paint { border: 2px solid red; border-style: none; } } @layer theme { #blocked { border-style: none; } #exposed { border-style: none; border-style: revert-layer; } #repeated { border-style: none; border-style: revert-layer; border-style: revert-layer; } #invalid { border-style: none; border-style: invalid; } #physical { border-style: none dashed dotted solid; } } #blocked { border-style: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let blocked = node("<div id='blocked'>Blocked</div>");
        let exposed = node("<div id='exposed'>Exposed</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let physical = node("<div id='physical'>Physical</div>");
        let style_only = node("<div id='style-only'>Style only</div>");
        let inline = node("<div id='inline' style='border-style:none'>Inline</div>");
        let inline_paint =
            node("<div id='inline-paint' style='border-style:solid'>Inline paint</div>");

        assert_eq!(stylesheet.computed_for(&blocked).border, None);
        assert_eq!(
            stylesheet.computed_for(&exposed).border.unwrap(),
            uniform_border(2, NativeColor::RED)
        );
        assert_eq!(
            stylesheet.computed_for(&repeated).border.unwrap(),
            uniform_border(2, NativeColor::RED)
        );
        assert_eq!(stylesheet.computed_for(&invalid).border, None);
        assert_eq!(stylesheet.computed_for(&style_only).border, None);
        assert_eq!(stylesheet.computed_for(&inline).border, None);
        assert_eq!(
            stylesheet.computed_for(&inline_paint).border.unwrap(),
            uniform_border(2, NativeColor::RED)
        );

        let physical_border = stylesheet.computed_for(&physical).border.unwrap();
        assert_eq!(physical_border.top().width(), 0);
        assert_eq!(
            physical_border.right(),
            styled_border_side(2, NativeBorderStyle::Dashed, NativeColor::RED)
        );
        assert_eq!(
            physical_border.bottom(),
            styled_border_side(2, NativeBorderStyle::Dotted, NativeColor::RED)
        );
        assert_eq!(
            physical_border.left(),
            styled_border_side(2, NativeBorderStyle::Solid, NativeColor::RED)
        );
    }

    #[test]
    fn stylesheet_cascade_layers_precede_specificity_and_reopen_by_source_order() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #target { color: red; } } @layer theme { .target { color: blue; } }"
                .into(),
            "@layer base { #target { color: green; } }".into(),
        ])
        .unwrap();
        let target = node("<div id='target' class='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&target).color(),
            Some(NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            })
        );

        let reopened = NativeStylesheet::from_sources(vec![
            "@layer base { #target { color: red; } } @layer base { #target { color: green; } }"
                .into(),
        ])
        .unwrap();
        assert_eq!(
            reopened.computed_for(&target).color(),
            Some(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            })
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_independent_paint_colors() {
        let blue = NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        };
        let green = NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        };
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { color: red; background-color: red; } #rollback { color: red; background-color: red; } #repeated { color: red; background-color: red; } #unlayered { color: red; background-color: red; } #inline { color: red; background-color: red; } #independent { color: red; background-color: red; } #parent { color: red; } } @layer theme { .named { color: blue; background-color: blue; } #rollback { color: revert-layer; background-color: revert-layer; } #repeated { color: revert-layer; background-color: revert-layer; } #unlayered { color: blue; background-color: blue; } #inline { color: blue; background-color: blue; } #independent { color: revert-layer; background-color: blue; } #parent { color: blue; } } @layer top { #repeated { color: revert-layer; background-color: revert-layer; } } #named { color: revert-layer; background-color: revert-layer; } #unlayered { color: revert-layer; background-color: revert-layer; } #parent { color: revert-layer; } #fallback { color: revert-layer; background-color: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let unlayered = node("<div id='unlayered'>Unlayered</div>");
        let inline = node(
            "<div id='inline' style='color:revert-layer;background-color:revert-layer'>Inline</div>",
        );
        let independent = node("<div id='independent'>Independent</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let parent = node("<div id='parent'>Parent</div>");

        let colors = |value: &NativeNode| {
            let style = stylesheet.computed_for(value);
            (style.color(), style.background_color())
        };
        assert_eq!(colors(&named), (Some(blue), Some(blue)));
        assert_eq!(
            colors(&rollback),
            (Some(NativeColor::RED), Some(NativeColor::RED))
        );
        assert_eq!(
            colors(&repeated),
            (Some(NativeColor::RED), Some(NativeColor::RED))
        );
        assert_eq!(colors(&unlayered), (Some(blue), Some(blue)));
        assert_eq!(colors(&inline), (Some(blue), Some(blue)));
        assert_eq!(colors(&independent), (Some(NativeColor::RED), Some(blue)));
        assert_eq!(colors(&fallback), (None, None));
        assert_eq!(colors(&parent), (Some(blue), None));

        let inherited = stylesheet.computed_for_with_matcher(
            &node("<div style='color:revert-layer'>Inherited</div>"),
            NativeInheritedStyle {
                color: Some(green),
                ..NativeInheritedStyle::default()
            },
            |_| false,
        );
        assert_eq!(inherited.color(), Some(green));
        assert_eq!(inherited.background_color(), None);
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_named_and_unlayered_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-skip-spaces: all; } #unlayered { text-decoration-skip-spaces: all; } } @layer theme { .named { text-decoration-skip-spaces: end; } #unlayered { text-decoration-skip-spaces: end; } } #named { text-decoration-skip-spaces: revert-layer; } #unlayered { text-decoration-skip-spaces: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let unlayered = node(
            "<div id='unlayered' style='text-decoration-skip-spaces:revert-layer'>Unlayered</div>",
        );
        assert_eq!(
            stylesheet
                .computed_for(&named)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::End
        );
        assert_eq!(
            stylesheet
                .computed_for(&unlayered)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::End
        );

        let repeated = NativeStylesheet::from_sources(vec![
            "@layer base { #target { text-decoration-skip-spaces: all; } } @layer theme { #target { text-decoration-skip-spaces: revert-layer; } } @layer top { #target { text-decoration-skip-spaces: revert-layer; } }"
                .into(),
        ])
        .unwrap();
        let target = node("<div id='target'>Target</div>");
        assert_eq!(
            repeated.computed_for(&target).text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::All
        );
    }

    #[test]
    fn stylesheet_cascade_layers_bound_invalid_forms_and_layer_count() {
        let mut source = String::from(
            "@layer { #anonymous { color: red; } } @layer base, theme { #comma { color: red; } } @layer outer { @layer inner { #nested { color: red; } } } @layer statement;",
        );
        for index in 0..=MAX_NATIVE_NAMED_CASCADE_LAYERS {
            source.push_str(&format!(
                " @layer limit-{index} {{ #limit-{index} {{ color: red; }} }}"
            ));
        }
        let mut diagnostics = NativeDiagnosticSink::default();
        let stylesheet =
            NativeStylesheet::from_sources_with_diagnostics(vec![source], &mut diagnostics)
                .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.detail == "layer-name")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.detail == "layer-prelude")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.detail == "nested-layer")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.detail == "layer-statement")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.detail == "too-many-layers")
        );
        assert_eq!(
            stylesheet.rules.len(),
            MAX_NATIVE_NAMED_CASCADE_LAYERS.saturating_sub(1)
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_skip_ink_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-skip-ink: none; } #unlayered { text-decoration-skip-ink: auto; } } @layer theme { .named { text-decoration-skip-ink: auto; } #unlayered { text-decoration-skip-ink: none; } } #named { text-decoration-skip-ink: revert-layer; } #unlayered { text-decoration-skip-ink: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let unlayered = node(
            "<div id='unlayered' style='text-decoration-skip-ink:revert-layer'>Unlayered</div>",
        );
        assert_eq!(
            stylesheet.computed_for(&named).text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(
            stylesheet
                .computed_for(&unlayered)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::None
        );

        let repeated = NativeStylesheet::from_sources(vec![
            "@layer base { #target { text-decoration-skip-ink: none; } } @layer theme { #target { text-decoration-skip-ink: auto; } } @layer top { #target { text-decoration-skip-ink: revert-layer; } }"
                .into(),
        ])
        .unwrap();
        let target = node("<div id='target'>Target</div>");
        assert_eq!(
            repeated.computed_for(&target).text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );

        let root_fallback = NativeStylesheet::from_sources(vec![
            "@layer base { #target { text-decoration-skip-ink: revert-layer; } }".into(),
        ])
        .unwrap();
        assert_eq!(
            root_fallback
                .computed_for(&target)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_style_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-style: dashed; } #rollback { text-decoration-style: solid; } #repeated { text-decoration-style: solid; } #inline { text-decoration-style: dotted; } #fallback { text-decoration-style: revert-layer; } } @layer theme { .named { text-decoration-style: dotted; } #rollback { text-decoration-style: double; } #repeated { text-decoration-style: revert-layer; } #inline { text-decoration-style: wavy; } } @layer top { #repeated { text-decoration-style: revert-layer; } } #named { text-decoration-style: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let inline =
            node("<div id='inline' style='text-decoration-style:revert-layer'>Inline</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        assert_eq!(
            stylesheet.computed_for(&named).text_decoration_style(),
            NativeTextDecorationStyle::Dotted
        );
        assert_eq!(
            stylesheet.computed_for(&rollback).text_decoration_style(),
            NativeTextDecorationStyle::Double
        );
        assert_eq!(
            stylesheet.computed_for(&repeated).text_decoration_style(),
            NativeTextDecorationStyle::Solid
        );
        assert_eq!(
            stylesheet.computed_for(&inline).text_decoration_style(),
            NativeTextDecorationStyle::Wavy
        );
        assert_eq!(
            stylesheet.computed_for(&fallback).text_decoration_style(),
            NativeTextDecorationStyle::Solid
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_thickness_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-thickness: 1px; } #rollback { text-decoration-thickness: 1px; } #repeated { text-decoration-thickness: 1px; } #unlayered { text-decoration-thickness: 2px; } #inline { text-decoration-thickness: 1px; } #fallback { text-decoration-thickness: revert-layer; } } @layer theme { .named { text-decoration-thickness: 2px; } #rollback { text-decoration-thickness: 3px; } #repeated { text-decoration-thickness: revert-layer; } #unlayered { text-decoration-thickness: 4px; } #inline { text-decoration-thickness: 2px; } } @layer top { #repeated { text-decoration-thickness: revert-layer; } } #named { text-decoration-thickness: revert-layer; } #unlayered { text-decoration-thickness: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let unlayered = node("<div id='unlayered'>Unlayered</div>");
        let inline =
            node("<div id='inline' style='text-decoration-thickness:revert-layer'>Inline</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        assert_eq!(
            stylesheet.computed_for(&named).text_decoration_thickness(),
            2
        );
        assert_eq!(
            stylesheet
                .computed_for(&rollback)
                .text_decoration_thickness(),
            3
        );
        assert_eq!(
            stylesheet
                .computed_for(&repeated)
                .text_decoration_thickness(),
            1
        );
        assert_eq!(
            stylesheet
                .computed_for(&unlayered)
                .text_decoration_thickness(),
            4
        );
        assert_eq!(
            stylesheet.computed_for(&inline).text_decoration_thickness(),
            2
        );
        assert_eq!(
            stylesheet
                .computed_for(&fallback)
                .text_decoration_thickness(),
            1
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_text_underline_offset_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-underline-offset: -1px; } #rollback { text-underline-offset: -1px; } #repeated { text-underline-offset: -1px; } #unlayered { text-underline-offset: -1px; } #inline { text-underline-offset: -1px; } #fallback { text-underline-offset: revert-layer; } } @layer theme { .named { text-underline-offset: -2px; } #rollback { text-underline-offset: revert-layer; } #repeated { text-underline-offset: revert-layer; } #unlayered { text-underline-offset: 4px; } #inline { text-underline-offset: 2px; } } @layer top { #repeated { text-underline-offset: revert-layer; } } #named { text-underline-offset: revert-layer; } #unlayered { text-underline-offset: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let unlayered = node("<div id='unlayered'>Unlayered</div>");
        let inline =
            node("<div id='inline' style='text-underline-offset:revert-layer'>Inline</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        assert_eq!(stylesheet.computed_for(&named).text_underline_offset(), -2);
        assert_eq!(
            stylesheet.computed_for(&rollback).text_underline_offset(),
            -1
        );
        assert_eq!(
            stylesheet.computed_for(&repeated).text_underline_offset(),
            -1
        );
        assert_eq!(
            stylesheet.computed_for(&unlayered).text_underline_offset(),
            4
        );
        assert_eq!(stylesheet.computed_for(&inline).text_underline_offset(), 2);
        assert_eq!(
            stylesheet.computed_for(&fallback).text_underline_offset(),
            0
        );
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
    fn border_radius_revert_layer_rolls_back_named_and_inline_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border-radius: 1px; } #repeat { border-radius: 2px; } #fallback { border-radius: revert-layer; } #inline { border-radius: 3px; } } @layer theme { #named { border-radius: 4px; } #repeat { border-radius: revert-layer; } } @layer top { #repeat { border-radius: revert-layer; } } #named { border-radius: revert-layer; } #repeat { border-radius: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let repeat = node("<div id='repeat'>Repeat</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node("<div id='inline' style='border-radius:ReVeRt-LaYeR'>Inline</div>");

        let style = |node| stylesheet.computed_for(node).border_radius();
        assert_eq!(
            style(&named),
            NativeBorderRadius {
                top_left: 4,
                top_right: 4,
                bottom_right: 4,
                bottom_left: 4,
            }
        );
        assert_eq!(
            style(&repeat),
            NativeBorderRadius {
                top_left: 2,
                top_right: 2,
                bottom_right: 2,
                bottom_left: 2,
            }
        );
        assert_eq!(style(&fallback), NativeBorderRadius::default());
        assert_eq!(
            style(&inline),
            NativeBorderRadius {
                top_left: 3,
                top_right: 3,
                bottom_right: 3,
                bottom_left: 3,
            }
        );

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#target { border-radius: ReVeRt-LaYeR; } #other { border-radius: 1px revert-layer; }"
                    .into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                        && diagnostic.detail == "border-radius"
                })
                .count(),
            1
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_box_edges_per_physical_side() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { padding: 1px 2px; margin: 3px; } #card { padding-left: 5px; margin-top: 4px; }"
                .into(),
        ])
        .unwrap();
        let edge_node =
            node("<div id='card' style='padding-bottom:6px;margin-right:7px'>Card</div>");
        let style = stylesheet.computed_for(&edge_node);
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
        let auto_margin = style.margin_auto();
        assert!(!auto_margin.top());
        assert!(!auto_margin.right());
        assert!(!auto_margin.bottom());
        assert!(!auto_margin.left());

        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { margin: auto 2px; } #card { margin-left: 4px; }".into(),
        ])
        .unwrap();
        let auto_node = node("<div id='card' style='margin-top:3px;margin-bottom:auto'>Card</div>");
        let style = stylesheet.computed_for(&auto_node);
        assert_eq!(
            style.margin(),
            NativeBoxEdges {
                top: 3,
                right: 2,
                bottom: 0,
                left: 4,
            }
        );
        assert!(!style.margin_auto().top());
        assert!(!style.margin_auto().right());
        assert!(style.margin_auto().bottom());
        assert!(!style.margin_auto().left());
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
    fn line_height_revert_layer_rolls_back_named_and_inline_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { line-height: 16px; } #repeated { line-height: 20px; } #inline { line-height: 24px; } } @layer theme { #named { line-height: 28px; } #repeated { line-height: revert-layer; } #inline { line-height: 32px; } } @layer top { #repeated { line-height: revert-layer; } } #named { line-height: revert-layer; } #repeated { line-height: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let inline = node("<div id='inline' style='line-height:ReVeRt-LaYeR'>Inline</div>");
        let fallback = node("<div id='fallback' style='line-height:revert-layer'>Fallback</div>");

        assert_eq!(stylesheet.computed_for(&named).line_height(), Some(28));
        assert_eq!(stylesheet.computed_for(&repeated).line_height(), Some(20));
        assert_eq!(stylesheet.computed_for(&inline).line_height(), Some(32));
        assert_eq!(stylesheet.computed_for(&fallback).line_height(), None);

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec!["#target { line-height: revert-layer; }".into()],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "line-height"
        }));
    }

    #[test]
    fn direction_revert_layer_rolls_back_named_and_inline_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { direction: ltr; } #repeated { direction: ltr; } #inline { direction: ltr; } #fallback { direction: revert-layer; } } @layer theme { #named { direction: rtl; } #repeated { direction: revert-layer; } #inline { direction: rtl; } } @layer top { #repeated { direction: revert-layer; } } #named { direction: revert-layer; } #repeated { direction: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let inline = node("<div id='inline' style='direction:ReVeRt-LaYeR'>Inline</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");

        assert_eq!(
            stylesheet.computed_for(&named).direction(),
            DirectionValue::Rtl
        );
        assert_eq!(
            stylesheet.computed_for(&repeated).direction(),
            DirectionValue::Ltr
        );
        assert_eq!(
            stylesheet.computed_for(&inline).direction(),
            DirectionValue::Rtl
        );
        assert_eq!(
            stylesheet.computed_for(&fallback).direction(),
            DirectionValue::Ltr
        );

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec!["#target { direction: revert-layer; }".into()],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "direction"
        }));
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
        assert_eq!(
            declarations.overflow_x,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Clip))
        );
        assert_eq!(
            declarations.overflow_y,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Other))
        );
    }

    #[test]
    fn overflow_revert_layer_rolls_back_independent_axes_and_shorthand() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { overflow: hidden; } #axis { overflow-x: hidden; overflow-y: clip; } #repeat { overflow: hidden; } #fallback { overflow: revert-layer; } #inline { overflow: hidden; } #unsupported { overflow: visible; } } @layer theme { #named { overflow-x: clip; overflow-y: revert-layer; } #axis { overflow-x: revert-layer; overflow-y: hidden; } #repeat { overflow: revert-layer; } } @layer top { #named { overflow: revert-layer; } #repeat { overflow: revert-layer; } } #named { overflow: revert-layer; } #repeat { overflow: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let axis = node("<div id='axis'>Axis</div>");
        let repeat = node("<div id='repeat'>Repeat</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node("<div id='inline' style='overflow:ReVeRt-LaYeR'>Inline</div>");
        let unsupported = node("<div id='unsupported'>Unsupported</div>");

        let style = |node| stylesheet.computed_for(node);
        assert!(style(&named).overflow_clip_x());
        assert!(style(&named).overflow_clip_y());
        assert!(style(&axis).overflow_clip_x());
        assert!(style(&axis).overflow_clip_y());
        assert!(style(&repeat).overflow_clip_x());
        assert!(style(&repeat).overflow_clip_y());
        assert!(!style(&fallback).overflow_clip_x());
        assert!(!style(&fallback).overflow_clip_y());
        assert!(style(&inline).overflow_clip_x());
        assert!(style(&inline).overflow_clip_y());
        assert!(!style(&unsupported).overflow_clip_x());
        assert!(!style(&unsupported).overflow_clip_y());

        assert_eq!(
            parse_overflow_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_overflow_declaration("visible"),
            Some(LocalCascadeDeclaration::Value(OverflowValue::Other))
        );

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#target { overflow: ReVeRt-LaYeR; overflow-x: visible; overflow-y: clip; }".into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && matches!(diagnostic.detail.as_str(), "overflow" | "overflow-y")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "overflow-x"
        }));
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
    fn line_height_declaration_parser_accepts_only_standalone_case_insensitive_revert_layer() {
        assert_eq!(
            parse_line_height_declaration("ReVeRt-LaYeR"),
            Some(LineHeightDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_line_height_declaration("28px"),
            Some(LineHeightDeclaration::Value(28))
        );
        assert_eq!(
            parse_line_height_declaration(" REVERT-LAYER "),
            Some(LineHeightDeclaration::RevertLayer)
        );
        assert_eq!(parse_line_height_declaration("inherit"), None);
        assert_eq!(parse_line_height_declaration("0px"), None);
        assert_eq!(parse_line_height_declaration("revert-layer 28px"), None);
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
    fn gap_parser_accepts_one_or_two_bounded_non_negative_pixels() {
        assert_eq!(
            parse_declarations("gap: 16px").gap,
            Some(GapShorthandDeclaration::Value(NativeGapValue {
                row: 16,
                column: 16,
            }))
        );
        assert_eq!(
            parse_declarations("gap: 8px 16px").gap,
            Some(GapShorthandDeclaration::Value(NativeGapValue {
                row: 8,
                column: 16,
            }))
        );
        assert_eq!(
            parse_declarations("gap: 0px").gap,
            Some(GapShorthandDeclaration::Value(NativeGapValue {
                row: 0,
                column: 0,
            }))
        );
        assert_eq!(
            parse_declarations("gap: REVERT-LAYER").gap,
            Some(GapShorthandDeclaration::RevertLayer)
        );
        assert_eq!(parse_declarations("gap: -1px").gap, None);
        assert_eq!(parse_declarations("gap: 1px 2px 3px").gap, None);
        assert_eq!(parse_declarations("gap: revert-layer 1px").gap, None);
        assert_eq!(parse_declarations("gap: 1.5px").gap, None);
        assert_eq!(parse_declarations("gap: 2em").gap, None);
        assert_eq!(parse_declarations("gap: 50%").gap, None);
        assert_eq!(parse_declarations("gap: 20000px").gap, None);
    }

    #[test]
    fn row_gap_parser_accepts_only_bounded_non_negative_single_pixels() {
        assert_eq!(
            parse_declarations("row-gap: 16px").row_gap,
            Some(GapComponentDeclaration::Value(16))
        );
        assert_eq!(
            parse_declarations("row-gap: 0px").row_gap,
            Some(GapComponentDeclaration::Value(0))
        );
        assert_eq!(
            parse_declarations("row-gap: revert-LAYER").row_gap,
            Some(GapComponentDeclaration::RevertLayer)
        );
        assert_eq!(parse_declarations("row-gap: -1px").row_gap, None);
        assert_eq!(parse_declarations("row-gap: 1px 2px").row_gap, None);
        assert_eq!(
            parse_declarations("row-gap: revert-layer 1px").row_gap,
            None
        );
        assert_eq!(parse_declarations("row-gap: 1.5px").row_gap, None);
        assert_eq!(parse_declarations("row-gap: 2em").row_gap, None);
        assert_eq!(parse_declarations("row-gap: 50%").row_gap, None);
        assert_eq!(parse_declarations("row-gap: 20000px").row_gap, None);
    }

    #[test]
    fn column_gap_parser_accepts_only_bounded_non_negative_single_pixels() {
        assert_eq!(
            parse_declarations("column-gap: 16px").column_gap,
            Some(GapComponentDeclaration::Value(16))
        );
        assert_eq!(
            parse_declarations("column-gap: 0px").column_gap,
            Some(GapComponentDeclaration::Value(0))
        );
        assert_eq!(
            parse_declarations("column-gap: REVERT-layer").column_gap,
            Some(GapComponentDeclaration::RevertLayer)
        );
        assert_eq!(parse_declarations("column-gap: -1px").column_gap, None);
        assert_eq!(parse_declarations("column-gap: 1px 2px").column_gap, None);
        assert_eq!(
            parse_declarations("column-gap: revert-layer 1px").column_gap,
            None
        );
        assert_eq!(parse_declarations("column-gap: 1.5px").column_gap, None);
        assert_eq!(parse_declarations("column-gap: 2em").column_gap, None);
        assert_eq!(parse_declarations("column-gap: 50%").column_gap, None);
        assert_eq!(parse_declarations("column-gap: 20000px").column_gap, None);
    }

    #[test]
    fn flex_grow_parser_accepts_only_bounded_non_negative_integers() {
        assert_eq!(parse_flex_grow("0"), Some(0));
        assert_eq!(parse_flex_grow("1024"), Some(1024));
        assert_eq!(parse_flex_grow(" 12 "), Some(12));
        assert_eq!(parse_flex_grow("-1"), None);
        assert_eq!(parse_flex_grow("+1"), None);
        assert_eq!(parse_flex_grow("1.5"), None);
        assert_eq!(parse_flex_grow("1px"), None);
        assert_eq!(parse_flex_grow("1025"), None);
        assert_eq!(parse_flex_grow("1e2"), None);
    }

    #[test]
    fn flex_shrink_parser_accepts_only_bounded_non_negative_integers() {
        assert_eq!(parse_flex_shrink("0"), Some(0));
        assert_eq!(parse_flex_shrink("1024"), Some(1024));
        assert_eq!(parse_flex_shrink(" 12 "), Some(12));
        assert_eq!(parse_flex_shrink("-1"), None);
        assert_eq!(parse_flex_shrink("+1"), None);
        assert_eq!(parse_flex_shrink("1.5"), None);
        assert_eq!(parse_flex_shrink("1px"), None);
        assert_eq!(parse_flex_shrink("1025"), None);
        assert_eq!(parse_flex_shrink("1e2"), None);
    }

    #[test]
    fn flex_basis_parser_accepts_auto_and_bounded_non_negative_pixels() {
        assert_eq!(parse_flex_basis("auto"), Some(FlexBasisValue::Auto));
        assert_eq!(parse_flex_basis("AUTO"), Some(FlexBasisValue::Auto));
        assert_eq!(parse_flex_basis("0px"), Some(FlexBasisValue::Length(0)));
        assert_eq!(parse_flex_basis(" 12px "), Some(FlexBasisValue::Length(12)));
        assert_eq!(parse_flex_basis("-1px"), None);
        assert_eq!(parse_flex_basis("12"), None);
        assert_eq!(parse_flex_basis("1.5px"), None);
        assert_eq!(parse_flex_basis("2em"), None);
        assert_eq!(parse_flex_basis("50%"), None);
        assert_eq!(parse_flex_basis("calc(12px)"), None);
        assert_eq!(parse_flex_basis("content"), None);
        assert_eq!(parse_flex_basis("20000px"), None);
        assert_eq!(
            parse_declarations("flex-basis: 12px; flex-basis: 1.5px").flex_basis,
            Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(12)))
        );
    }

    #[test]
    fn flex_shorthand_parser_expands_bounded_forms_and_rejects_unsupported() {
        assert_eq!(
            parse_flex_shorthand("none"),
            Some((0, 0, FlexBasisValue::Auto))
        );
        assert_eq!(
            parse_flex_shorthand("AUTO"),
            Some((1, 1, FlexBasisValue::Auto))
        );
        assert_eq!(
            parse_flex_shorthand("2"),
            Some((2, 1, FlexBasisValue::Length(0)))
        );
        assert_eq!(
            parse_flex_shorthand("2 3"),
            Some((2, 3, FlexBasisValue::Length(0)))
        );
        assert_eq!(
            parse_flex_shorthand("2 auto"),
            Some((2, 1, FlexBasisValue::Auto))
        );
        assert_eq!(
            parse_flex_shorthand("2 12px"),
            Some((2, 1, FlexBasisValue::Length(12)))
        );
        assert_eq!(
            parse_flex_shorthand("2 3 auto"),
            Some((2, 3, FlexBasisValue::Auto))
        );
        assert_eq!(
            parse_flex_shorthand("2 3 12px"),
            Some((2, 3, FlexBasisValue::Length(12)))
        );
        assert_eq!(parse_flex_shorthand("initial"), None);
        assert_eq!(parse_flex_shorthand("inherit"), None);
        assert_eq!(parse_flex_shorthand("1.5"), None);
        assert_eq!(parse_flex_shorthand("-1"), None);
        assert_eq!(parse_flex_shorthand("1px"), None);
        assert_eq!(parse_flex_shorthand("1 2 3"), None);
        assert_eq!(parse_flex_shorthand("1 50%"), None);
        assert_eq!(parse_flex_shorthand("1 2 3%"), None);
        assert_eq!(parse_flex_shorthand("1 2 3 4px"), None);
        assert_eq!(
            parse_flex_shorthand_declaration("ReVeRt-LaYeR"),
            Some((
                FlexGrowDeclaration::RevertLayer,
                FlexShrinkDeclaration::RevertLayer,
                FlexBasisDeclaration::RevertLayer,
            ))
        );
        assert_eq!(parse_flex_shorthand_declaration("revert-layer 1"), None);
        assert_eq!(parse_flex_shorthand_declaration("initial"), None);
        assert_eq!(
            parse_declarations("flex: 2 3 12px; flex-grow: 4; flex-basis: auto"),
            NativeDeclarations {
                flex_grow: Some(FlexGrowDeclaration::Value(4)),
                flex_shrink: Some(FlexShrinkDeclaration::Value(3)),
                flex_basis: Some(FlexBasisDeclaration::Value(FlexBasisValue::Auto)),
                ..NativeDeclarations::default()
            }
        );
        assert_eq!(
            parse_declarations("flex: 2 3 12px; flex: revert-layer"),
            NativeDeclarations {
                flex_grow: Some(FlexGrowDeclaration::RevertLayer),
                flex_shrink: Some(FlexShrinkDeclaration::RevertLayer),
                flex_basis: Some(FlexBasisDeclaration::RevertLayer),
                ..NativeDeclarations::default()
            }
        );
        assert_eq!(
            parse_declarations("flex: revert-layer; flex-grow: 4"),
            NativeDeclarations {
                flex_grow: Some(FlexGrowDeclaration::Value(4)),
                flex_shrink: Some(FlexShrinkDeclaration::RevertLayer),
                flex_basis: Some(FlexBasisDeclaration::RevertLayer),
                ..NativeDeclarations::default()
            }
        );
    }

    #[test]
    fn flex_flow_parser_expands_bounded_direction_and_wrap_forms() {
        assert_eq!(
            parse_flex_flow("row"),
            Some((FlexDirectionValue::Row, FlexWrapValue::NoWrap))
        );
        assert_eq!(
            parse_flex_flow("row-reverse"),
            Some((FlexDirectionValue::RowReverse, FlexWrapValue::NoWrap))
        );
        assert_eq!(
            parse_flex_flow("column"),
            Some((FlexDirectionValue::Column, FlexWrapValue::NoWrap))
        );
        assert_eq!(
            parse_flex_flow("COLUMN-REVERSE"),
            Some((FlexDirectionValue::ColumnReverse, FlexWrapValue::NoWrap))
        );
        assert_eq!(
            parse_flex_flow("wrap"),
            Some((FlexDirectionValue::Row, FlexWrapValue::Wrap))
        );
        assert_eq!(
            parse_flex_flow("wrap-reverse"),
            Some((FlexDirectionValue::Row, FlexWrapValue::WrapReverse))
        );
        assert_eq!(
            parse_flex_flow("row-reverse wrap"),
            Some((FlexDirectionValue::RowReverse, FlexWrapValue::Wrap))
        );
        assert_eq!(
            parse_flex_flow("WRAP ROW"),
            Some((FlexDirectionValue::Row, FlexWrapValue::Wrap))
        );
        assert_eq!(
            parse_flex_flow("wrap-reverse row-reverse"),
            Some((FlexDirectionValue::RowReverse, FlexWrapValue::WrapReverse))
        );
        assert_eq!(parse_flex_flow(""), None);
        assert_eq!(
            parse_flex_flow("column wrap"),
            Some((FlexDirectionValue::Column, FlexWrapValue::Wrap))
        );
        assert_eq!(
            parse_flex_flow("column-reverse wrap-reverse"),
            Some((
                FlexDirectionValue::ColumnReverse,
                FlexWrapValue::WrapReverse
            ))
        );
        assert_eq!(parse_flex_flow("row column"), None);
        assert_eq!(parse_flex_flow("wrap wrap-reverse"), None);
        assert_eq!(parse_flex_flow("row row-reverse"), None);
        assert_eq!(parse_flex_flow("initial"), None);
        assert_eq!(
            parse_flex_flow_declaration("ReVeRt-LaYeR"),
            Some((
                FlexDirectionDeclaration::RevertLayer,
                FlexWrapDeclaration::RevertLayer,
            ))
        );
        assert_eq!(parse_flex_flow_declaration("revert-layer row"), None);
        assert_eq!(
            parse_declarations(
                "flex-flow: row-reverse wrap; flex-direction: row; flex-wrap: nowrap"
            ),
            NativeDeclarations {
                flex_direction: Some(FlexDirectionDeclaration::Value(FlexDirectionValue::Row)),
                flex_wrap: Some(FlexWrapDeclaration::Value(FlexWrapValue::NoWrap)),
                ..NativeDeclarations::default()
            }
        );
        assert_eq!(
            parse_declarations("flex-flow: revert-layer; flex-direction: column"),
            NativeDeclarations {
                flex_direction: Some(FlexDirectionDeclaration::Value(FlexDirectionValue::Column,)),
                flex_wrap: Some(FlexWrapDeclaration::RevertLayer),
                ..NativeDeclarations::default()
            }
        );
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
        assert_eq!(
            parse_opacity_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_opacity_declaration("50%"),
            Some(LocalCascadeDeclaration::Value(128))
        );
        for value in [
            "revert-layer 50%",
            "50% revert-layer",
            "inherit",
            "unset",
            "revert",
            "initial",
        ] {
            assert_eq!(parse_opacity_declaration(value), None, "value={value}");
        }
        assert_eq!(
            parse_declarations("opacity: 50%; opacity: 1.001").opacity,
            Some(LocalCascadeDeclaration::Value(128))
        );
        assert_eq!(
            parse_declarations("opacity: 50% !important").opacity,
            Some(LocalCascadeDeclaration::Value(128))
        );
    }

    #[test]
    fn opacity_revert_layer_rolls_back_named_and_inline_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { opacity: 25%; } #repeat { opacity: 50%; } #fallback { opacity: revert-layer; } #inline { opacity: 75%; } } @layer theme { #named { opacity: 75%; } #repeat { opacity: revert-layer; } } @layer top { #repeat { opacity: revert-layer; } } #named { opacity: revert-layer; } #repeat { opacity: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let repeat = node("<div id='repeat'>Repeat</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node("<div id='inline' style='opacity:ReVeRt-LaYeR'>Inline</div>");

        let opacity = |node: &NativeNode| stylesheet.computed_for(node).opacity();
        assert_eq!(opacity(&named), 191);
        assert_eq!(opacity(&repeat), 128);
        assert_eq!(opacity(&fallback), u8::MAX);
        assert_eq!(opacity(&inline), 191);

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec!["#target { opacity: ReVeRt-LaYeR; } #other { opacity: 50% revert-layer; }".into()],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                        && diagnostic.detail == "opacity"
                })
                .count(),
            1
        );
    }

    #[test]
    fn text_align_parser_accepts_bounded_physical_and_logical_values() {
        assert_eq!(parse_text_align("left"), Some(TextAlignValue::Left));
        assert_eq!(parse_text_align("CENTER"), Some(TextAlignValue::Center));
        assert_eq!(parse_text_align("right"), Some(TextAlignValue::Right));
        assert_eq!(parse_text_align("START"), Some(TextAlignValue::Start));
        assert_eq!(parse_text_align("end"), Some(TextAlignValue::End));
        assert_eq!(parse_text_align("JUSTIFY"), Some(TextAlignValue::Justify));
        assert_eq!(parse_text_align("match-parent"), None);
        assert_eq!(parse_text_align("start end"), None);
    }

    #[test]
    fn text_align_last_parser_accepts_bounded_inherited_values() {
        assert_eq!(
            parse_text_align_last("auto"),
            Some(TextAlignLastValue::Auto)
        );
        assert_eq!(
            parse_text_align_last("LEFT"),
            Some(TextAlignLastValue::Left)
        );
        assert_eq!(
            parse_text_align_last("center"),
            Some(TextAlignLastValue::Center)
        );
        assert_eq!(
            parse_text_align_last("right"),
            Some(TextAlignLastValue::Right)
        );
        assert_eq!(
            parse_text_align_last("START"),
            Some(TextAlignLastValue::Start)
        );
        assert_eq!(parse_text_align_last("end"), Some(TextAlignLastValue::End));
        assert_eq!(
            parse_text_align_last("JUSTIFY"),
            Some(TextAlignLastValue::Justify)
        );
        assert_eq!(parse_text_align_last("match-parent"), None);
        assert_eq!(parse_text_align_last("start end"), None);
    }

    #[test]
    fn text_justify_parser_accepts_bounded_inherited_values() {
        assert_eq!(parse_text_justify("auto"), Some(TextJustifyValue::Auto));
        assert_eq!(parse_text_justify("NONE"), Some(TextJustifyValue::None));
        assert_eq!(
            parse_text_justify("inter-word"),
            Some(TextJustifyValue::InterWord)
        );
        assert_eq!(parse_text_justify("inter-character"), None);
        assert_eq!(parse_text_justify("distribute"), None);
        assert_eq!(parse_text_justify("auto none"), None);
    }

    #[test]
    fn alignment_declaration_parser_accepts_only_standalone_case_insensitive_revert_layer() {
        assert_eq!(
            parse_text_align_declaration("ReVeRt-LaYeR"),
            Some(TextAlignDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_align_last_declaration(" REVERT-LAYER "),
            Some(TextAlignLastDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_justify_declaration("revert-LAYER"),
            Some(TextJustifyDeclaration::RevertLayer)
        );
        assert_eq!(parse_text_align_declaration("center revert-layer"), None);
        assert_eq!(parse_text_align_last_declaration("inherit"), None);
        assert_eq!(parse_text_justify_declaration("none revert-layer"), None);
    }

    #[test]
    fn white_space_declaration_parser_accepts_bounded_modes_and_revert_layer() {
        assert_eq!(
            parse_white_space_declaration("ReVeRt-LaYeR"),
            Some(WhiteSpaceDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_white_space_declaration("PRE-WRAP"),
            Some(WhiteSpaceDeclaration::Value(WhiteSpaceValue::PreWrap))
        );
        assert_eq!(
            parse_white_space_declaration("pre-line"),
            Some(WhiteSpaceDeclaration::Value(WhiteSpaceValue::PreLine))
        );
        assert_eq!(parse_white_space_declaration("inherit"), None);
        assert_eq!(parse_white_space_declaration("pre wrap"), None);
        assert_eq!(parse_white_space_declaration("revert-layer pre"), None);
    }

    #[test]
    fn direction_parser_accepts_only_bounded_inherited_values() {
        assert_eq!(parse_direction("ltr"), Some(DirectionValue::Ltr));
        assert_eq!(parse_direction("RTL"), Some(DirectionValue::Rtl));
        assert_eq!(parse_direction("inherit"), None);
        assert_eq!(parse_direction("vertical-rl"), None);
        assert_eq!(parse_direction("rtl ltr"), None);
    }

    #[test]
    fn direction_declaration_parser_accepts_only_standalone_case_insensitive_revert_layer() {
        assert_eq!(
            parse_direction_declaration("ReVeRt-LaYeR"),
            Some(DirectionDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_direction_declaration("RTL"),
            Some(DirectionDeclaration::Value(DirectionValue::Rtl))
        );
        assert_eq!(
            parse_direction_declaration(" REVERT-LAYER "),
            Some(DirectionDeclaration::RevertLayer)
        );
        assert_eq!(parse_direction_declaration("inherit"), None);
        assert_eq!(parse_direction_declaration("vertical-rl"), None);
        assert_eq!(parse_direction_declaration("revert-layer rtl"), None);
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
        assert_eq!(
            parse_justify_content("SPACE-AROUND"),
            Some(JustifyContentValue::SpaceAround)
        );
        assert_eq!(
            parse_justify_content("SPACE-EVENLY"),
            Some(JustifyContentValue::SpaceEvenly)
        );
        assert_eq!(
            parse_justify_content("NORMAL"),
            Some(JustifyContentValue::Normal)
        );
        assert_eq!(
            parse_justify_content("STRETCH"),
            Some(JustifyContentValue::Stretch)
        );
        assert_eq!(parse_justify_content("safe center"), None);
        assert_eq!(parse_justify_content("start"), None);
    }

    #[test]
    fn place_content_parser_expands_bounded_shared_and_axis_values() {
        assert_eq!(
            parse_place_content("center"),
            Some((AlignContentValue::Center, JustifyContentValue::Center))
        );
        assert_eq!(
            parse_place_content("SPACE-BETWEEN"),
            Some((
                AlignContentValue::SpaceBetween,
                JustifyContentValue::SpaceBetween
            ))
        );
        assert_eq!(
            parse_place_content("space-around flex-end"),
            Some((AlignContentValue::SpaceAround, JustifyContentValue::FlexEnd))
        );
        assert_eq!(
            parse_place_content("space-around"),
            Some((
                AlignContentValue::SpaceAround,
                JustifyContentValue::SpaceAround
            ))
        );
        assert_eq!(
            parse_place_content("space-evenly"),
            Some((
                AlignContentValue::SpaceEvenly,
                JustifyContentValue::SpaceEvenly
            ))
        );
        assert_eq!(
            parse_place_content("normal"),
            Some((AlignContentValue::Normal, JustifyContentValue::Normal))
        );
        assert_eq!(
            parse_place_content("stretch"),
            Some((AlignContentValue::Stretch, JustifyContentValue::Stretch))
        );
        assert_eq!(
            parse_place_content("normal center"),
            Some((AlignContentValue::Normal, JustifyContentValue::Center))
        );
        assert_eq!(
            parse_place_content("stretch flex-start"),
            Some((AlignContentValue::Stretch, JustifyContentValue::FlexStart))
        );
        assert_eq!(
            parse_place_content("center stretch"),
            Some((AlignContentValue::Center, JustifyContentValue::Stretch))
        );
        assert_eq!(parse_place_content("center center center"), None);
        assert_eq!(parse_place_content("start center"), None);
        assert_eq!(
            parse_place_content_declaration("ReVeRt-LaYeR"),
            Some((
                AlignContentDeclaration::RevertLayer,
                JustifyContentDeclaration::RevertLayer,
            ))
        );
        assert_eq!(parse_place_content_declaration("revert-layer center"), None);
        assert_eq!(
            parse_declarations("place-content: revert-layer; justify-content: flex-end"),
            NativeDeclarations {
                align_content: Some(AlignContentDeclaration::RevertLayer),
                justify_content: Some(JustifyContentDeclaration::Value(
                    JustifyContentValue::FlexEnd,
                )),
                ..NativeDeclarations::default()
            }
        );
    }

    #[test]
    fn align_items_parser_accepts_only_bounded_cross_axis_values() {
        assert_eq!(
            parse_align_items("flex-start"),
            Some(AlignItemsValue::FlexStart)
        );
        assert_eq!(parse_align_items("CENTER"), Some(AlignItemsValue::Center));
        assert_eq!(
            parse_align_items("flex-end"),
            Some(AlignItemsValue::FlexEnd)
        );
        assert_eq!(parse_align_items("stretch"), Some(AlignItemsValue::Stretch));
        assert_eq!(parse_align_items("NORMAL"), Some(AlignItemsValue::Normal));
        assert_eq!(parse_align_items("baseline"), None);
        assert_eq!(parse_align_items("start"), None);
        assert_eq!(parse_align_items("end"), None);
    }

    #[test]
    fn align_self_parser_accepts_only_bounded_item_values() {
        assert_eq!(parse_align_self("auto"), Some(AlignSelfValue::Auto));
        assert_eq!(
            parse_align_self("FLEX-START"),
            Some(AlignSelfValue::FlexStart)
        );
        assert_eq!(parse_align_self("center"), Some(AlignSelfValue::Center));
        assert_eq!(parse_align_self("flex-end"), Some(AlignSelfValue::FlexEnd));
        assert_eq!(parse_align_self("stretch"), Some(AlignSelfValue::Stretch));
        assert_eq!(parse_align_self("NORMAL"), Some(AlignSelfValue::Normal));
        assert_eq!(parse_align_self("baseline"), None);
        assert_eq!(parse_align_self("safe center"), None);
        assert_eq!(parse_align_self("flex-start center"), None);
    }

    #[test]
    fn align_content_parser_accepts_only_bounded_line_distribution_values() {
        assert_eq!(
            parse_align_content("flex-start"),
            Some(AlignContentValue::FlexStart)
        );
        assert_eq!(
            parse_align_content("CENTER"),
            Some(AlignContentValue::Center)
        );
        assert_eq!(
            parse_align_content("flex-end"),
            Some(AlignContentValue::FlexEnd)
        );
        assert_eq!(
            parse_align_content("space-between"),
            Some(AlignContentValue::SpaceBetween)
        );
        assert_eq!(
            parse_align_content("space-around"),
            Some(AlignContentValue::SpaceAround)
        );
        assert_eq!(
            parse_align_content("SPACE-EVENLY"),
            Some(AlignContentValue::SpaceEvenly)
        );
        assert_eq!(
            parse_align_content("STRETCH"),
            Some(AlignContentValue::Stretch)
        );
        assert_eq!(
            parse_align_content("NORMAL"),
            Some(AlignContentValue::Normal)
        );
        assert_eq!(parse_align_content("start"), None);
        assert_eq!(parse_align_content("safe center"), None);
    }

    #[test]
    fn flex_direction_parser_accepts_bounded_row_and_column_values() {
        assert_eq!(parse_flex_direction("row"), Some(FlexDirectionValue::Row));
        assert_eq!(
            parse_flex_direction("ROW-REVERSE"),
            Some(FlexDirectionValue::RowReverse)
        );
        assert_eq!(
            parse_flex_direction("COLUMN"),
            Some(FlexDirectionValue::Column)
        );
        assert_eq!(
            parse_flex_direction("column-reverse"),
            Some(FlexDirectionValue::ColumnReverse)
        );
        assert_eq!(parse_flex_direction("start"), None);
        assert_eq!(parse_flex_direction("row reverse"), None);
    }

    #[test]
    fn flex_direction_declaration_parser_accepts_only_standalone_case_insensitive_revert_layer() {
        assert_eq!(
            parse_flex_direction_declaration("ReVeRt-LaYeR"),
            Some(FlexDirectionDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_flex_direction_declaration("COLUMN-REVERSE"),
            Some(FlexDirectionDeclaration::Value(
                FlexDirectionValue::ColumnReverse
            ))
        );
        assert_eq!(
            parse_flex_direction_declaration(" REVERT-LAYER "),
            Some(FlexDirectionDeclaration::RevertLayer)
        );
        assert_eq!(parse_flex_direction_declaration("inherit"), None);
        assert_eq!(parse_flex_direction_declaration("revert-layer row"), None);
        assert_eq!(parse_flex_direction_declaration("row reverse"), None);
    }

    #[test]
    fn flex_wrap_parser_accepts_only_bounded_line_values() {
        assert_eq!(parse_flex_wrap("nowrap"), Some(FlexWrapValue::NoWrap));
        assert_eq!(parse_flex_wrap("WRAP"), Some(FlexWrapValue::Wrap));
        assert_eq!(
            parse_flex_wrap("WRAP-REVERSE"),
            Some(FlexWrapValue::WrapReverse)
        );
        assert_eq!(parse_flex_wrap("row"), None);
        assert_eq!(parse_flex_wrap("normal"), None);
    }

    #[test]
    fn flexbox_declaration_parsers_accept_only_standalone_case_insensitive_revert_layer() {
        assert_eq!(
            parse_justify_content_declaration("ReVeRt-LaYeR"),
            Some(JustifyContentDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_align_items_declaration(" REVERT-LAYER "),
            Some(AlignItemsDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_align_self_declaration("revert-layer"),
            Some(AlignSelfDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_align_content_declaration("ReVeRt-LaYeR"),
            Some(AlignContentDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_flex_wrap_declaration("REVERT-LAYER"),
            Some(FlexWrapDeclaration::RevertLayer)
        );
        assert_eq!(parse_justify_content_declaration("safe center"), None);
        assert_eq!(parse_align_items_declaration("inherit"), None);
        assert_eq!(parse_align_self_declaration("center flex-end"), None);
        assert_eq!(parse_align_content_declaration("revert-layer center"), None);
        assert_eq!(parse_flex_wrap_declaration("wrap reverse"), None);
    }

    #[test]
    fn flex_item_order_parser_accepts_only_bounded_signed_integers() {
        assert_eq!(
            parse_flex_item_order("-1024"),
            Some(NativeOrderValue(-1024))
        );
        assert_eq!(parse_flex_item_order("+1024"), Some(NativeOrderValue(1024)));
        assert_eq!(
            parse_flex_item_order("  -12  "),
            Some(NativeOrderValue(-12))
        );
        assert_eq!(parse_flex_item_order("1025"), None);
        assert_eq!(parse_flex_item_order("-1025"), None);
        assert_eq!(parse_flex_item_order("1.0"), None);
        assert_eq!(parse_flex_item_order("1e1"), None);
        assert_eq!(parse_flex_item_order("--1"), None);
        assert_eq!(parse_flex_item_order("+ 1"), None);
        assert_eq!(parse_flex_item_order("normal"), None);
        assert_eq!(parse_flex_item_order("1px"), None);
    }

    #[test]
    fn flex_item_declaration_parsers_accept_only_standalone_case_insensitive_revert_layer() {
        assert_eq!(
            parse_flex_item_order_declaration("ReVeRt-LaYeR"),
            Some(FlexItemOrderDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_flex_item_order_declaration("-12"),
            Some(FlexItemOrderDeclaration::Value(NativeOrderValue(-12)))
        );
        assert_eq!(
            parse_flex_grow_declaration(" REVERT-LAYER "),
            Some(FlexGrowDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_flex_grow_declaration("2"),
            Some(FlexGrowDeclaration::Value(2))
        );
        assert_eq!(
            parse_flex_shrink_declaration("revert-LAYER"),
            Some(FlexShrinkDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_flex_shrink_declaration("3"),
            Some(FlexShrinkDeclaration::Value(3))
        );
        assert_eq!(
            parse_flex_basis_declaration("REVERT-LAYER"),
            Some(FlexBasisDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_flex_basis_declaration("12px"),
            Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(12)))
        );

        assert_eq!(parse_flex_item_order_declaration("inherit"), None);
        assert_eq!(parse_flex_grow_declaration("initial"), None);
        assert_eq!(parse_flex_shrink_declaration("revert"), None);
        assert_eq!(parse_flex_basis_declaration("unset"), None);
        assert_eq!(parse_flex_item_order_declaration("revert-layer -1"), None);
        assert_eq!(parse_flex_grow_declaration("1 revert-layer"), None);
        assert_eq!(parse_flex_shrink_declaration("1.5"), None);
        assert_eq!(parse_flex_basis_declaration("50%"), None);
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_flex_item_order_and_sizing_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { order: -1; flex-grow: 1; flex-shrink: 2; flex-basis: 4px; } #rollback { order: -2; flex-grow: 2; flex-shrink: 3; flex-basis: 6px; } #repeated { order: -3; flex-grow: 3; flex-shrink: 4; flex-basis: 8px; } #unlayered { order: -4; flex-grow: 4; flex-shrink: 5; flex-basis: 10px; } #inline { order: -5; flex-grow: 5; flex-shrink: 6; flex-basis: 12px; } #fallback { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } } @layer theme { .named { order: 1; flex-grow: 11; flex-shrink: 12; flex-basis: 14px; } #rollback { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } #repeated { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } #unlayered { order: 2; flex-grow: 13; flex-shrink: 14; flex-basis: 16px; } #inline { order: 3; flex-grow: 15; flex-shrink: 16; flex-basis: 18px; } } @layer top { #repeated { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } } #named { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } #unlayered { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let unlayered = node("<div id='unlayered'>Unlayered</div>");
        let inline = node(
            "<div id='inline' style='order:revert-layer;flex-grow:revert-layer;flex-shrink:revert-layer;flex-basis:revert-layer'>Inline</div>",
        );
        let fallback = node("<div id='fallback'>Fallback</div>");
        let parent = node(
            "<div id='parent' style='order:7;flex-grow:7;flex-shrink:7;flex-basis:20px'><span id='child'>Child</span></div>",
        );
        let child = node("<span id='child'>Child</span>");

        let assert_values =
            |element: &NativeNode, order: i32, grow: u32, shrink: u32, basis: FlexBasisValue| {
                let style = stylesheet.computed_for(element);
                assert_eq!(style.flex_item_order(), NativeOrderValue(order));
                assert_eq!(style.flex_grow(), grow);
                assert_eq!(style.flex_shrink(), shrink);
                assert_eq!(style.flex_basis(), basis);
            };

        assert_values(&named, 1, 11, 12, FlexBasisValue::Length(14));
        assert_values(&rollback, -2, 2, 3, FlexBasisValue::Length(6));
        assert_values(&repeated, -3, 3, 4, FlexBasisValue::Length(8));
        assert_values(&unlayered, 2, 13, 14, FlexBasisValue::Length(16));
        assert_values(&inline, 3, 15, 16, FlexBasisValue::Length(18));
        assert_values(&fallback, 0, 0, 1, FlexBasisValue::Auto);

        let parent_style = stylesheet.computed_for(&parent);
        assert_eq!(parent_style.flex_item_order(), NativeOrderValue(7));
        assert_eq!(parent_style.flex_grow(), 7);
        assert_eq!(parent_style.flex_shrink(), 7);
        assert_eq!(parent_style.flex_basis(), FlexBasisValue::Length(20));
        assert_values(&child, 0, 0, 1, FlexBasisValue::Auto);
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_flex_shorthand_components() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #first { flex: 0 1 4px; } #repeated { flex: 2 3 7px; } #mixed { flex: 2 3 6px; } } @layer theme { #first { flex: 1 0 10px; } #repeated { flex: 4 2 9px; } #mixed { flex: revert-layer; flex-grow: 5; } } @layer top { #repeated { flex: revert-layer; } } #first { flex: revert-layer; } #repeated { flex: revert-layer; } #mixed { flex: revert-layer; } #fallback { flex: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let first = node("<div id='first'>First</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let mixed = node("<div id='mixed'>Mixed</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node("<div id='first' style='flex:REVERT-LAYER'>Inline</div>");

        let assert_values =
            |element: &NativeNode, grow: u32, shrink: u32, basis: FlexBasisValue| {
                let style = stylesheet.computed_for(element);
                assert_eq!(style.flex_grow(), grow);
                assert_eq!(style.flex_shrink(), shrink);
                assert_eq!(style.flex_basis(), basis);
            };

        assert_values(&first, 1, 0, FlexBasisValue::Length(10));
        assert_values(&repeated, 4, 2, FlexBasisValue::Length(9));
        assert_values(&mixed, 5, 3, FlexBasisValue::Length(6));
        assert_values(&fallback, 0, 1, FlexBasisValue::Auto);
        assert_values(&inline, 1, 0, FlexBasisValue::Length(10));
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_flex_flow_and_place_content_components() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #flow { flex-flow: row nowrap; } #content { place-content: flex-start; } #mixed { flex-flow: row nowrap; place-content: flex-start; } #inline { flex-flow: row nowrap; place-content: flex-start; } } @layer theme { #flow { flex-flow: column wrap; } #content { place-content: space-between flex-end; } #mixed { flex-flow: revert-layer; flex-direction: row-reverse; place-content: revert-layer; justify-content: space-around; } #inline { flex-flow: column wrap; place-content: center flex-end; } } @layer top { #flow { flex-flow: revert-layer; } #content { place-content: revert-layer; } #mixed { flex-flow: column-reverse wrap; flex-flow: revert-layer; place-content: center; place-content: revert-layer; } } #flow { flex-flow: revert-layer; } #content { place-content: revert-layer; } #mixed { flex-flow: revert-layer; place-content: revert-layer; } #fallback { flex-flow: revert-layer; place-content: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let flow = node("<div id='flow'>Flow</div>");
        let content = node("<div id='content'>Content</div>");
        let mixed = node("<div id='mixed'>Mixed</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node(
            "<div id='inline' style='flex-flow:REVERT-LAYER;place-content:REVERT-LAYER'>Inline</div>",
        );

        let assert_values = |element: &NativeNode,
                             direction: FlexDirectionValue,
                             wrap: FlexWrapValue,
                             align_content: AlignContentValue,
                             justify_content: JustifyContentValue| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.flex_direction(), direction);
            assert_eq!(style.flex_wrap(), wrap);
            assert_eq!(style.align_content(), align_content);
            assert_eq!(style.justify_content(), justify_content);
        };

        assert_values(
            &flow,
            FlexDirectionValue::Column,
            FlexWrapValue::Wrap,
            AlignContentValue::FlexStart,
            JustifyContentValue::FlexStart,
        );
        assert_values(
            &content,
            FlexDirectionValue::Row,
            FlexWrapValue::NoWrap,
            AlignContentValue::SpaceBetween,
            JustifyContentValue::FlexEnd,
        );
        assert_values(
            &mixed,
            FlexDirectionValue::RowReverse,
            FlexWrapValue::NoWrap,
            AlignContentValue::FlexStart,
            JustifyContentValue::SpaceAround,
        );
        assert_values(
            &fallback,
            FlexDirectionValue::Row,
            FlexWrapValue::NoWrap,
            AlignContentValue::FlexStart,
            JustifyContentValue::FlexStart,
        );
        assert_values(
            &inline,
            FlexDirectionValue::Column,
            FlexWrapValue::Wrap,
            AlignContentValue::Center,
            JustifyContentValue::FlexEnd,
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_gap_components() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { gap: 2px 3px; } #rollback { gap: 6px 7px; } #repeated { gap: 10px 11px; } #fallback { gap: revert-layer; } #inline { gap: 18px 19px; } #invalid { gap: 24px 25px; gap: 1px 2px 3px; row-gap: 26px; row-gap: revert-layer 1px; } #longhand { row-gap: 2px; column-gap: 3px; } #mixed { gap: 30px 31px; } } @layer theme { .named { gap: 4px 5px; } #rollback { gap: 8px 9px; } #repeated { gap: 12px 13px; } #inline { gap: 20px 21px; } #longhand { row-gap: 4px; column-gap: 5px; } #mixed { gap: 40px 41px; } } @layer top { #repeated { gap: revert-layer; } #longhand { row-gap: revert-layer; column-gap: revert-layer; } } #named { gap: revert-layer; } #rollback { gap: revert-layer; row-gap: 10px; } #repeated { gap: revert-layer; } #mixed { gap: revert-layer; column-gap: 42px; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let inline = node("<div id='inline' style='gap:revert-layer;column-gap:22px'>Inline</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let longhand = node("<div id='longhand'>Longhand</div>");
        let mixed = node("<div id='mixed'>Mixed</div>");

        let assert_values = |element: &NativeNode, row: u32, column: u32| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.row_gap(), row);
            assert_eq!(style.column_gap(), column);
        };

        assert_values(&named, 4, 5);
        assert_values(&rollback, 10, 9);
        assert_values(&repeated, 12, 13);
        assert_values(&fallback, 0, 0);
        assert_values(&inline, 20, 22);
        assert_values(&invalid, 26, 25);
        assert_values(&longhand, 4, 5);
        assert_values(&mixed, 40, 42);
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_inherited_text_presentation() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #transform { text-transform: lowercase; } #weight { font-weight: normal; } #style { font-style: normal; } #break { word-break: normal; } #repeated { text-transform: lowercase; } #fallback { text-transform: revert-layer; } } @layer theme { #transform { text-transform: uppercase; } #weight { font-weight: bold; } #style { font-style: italic; } #break { word-break: break-all; } #repeated { text-transform: revert-layer; } #fallback { text-transform: revert-layer; } } @layer top { #repeated { text-transform: revert-layer; } } #transform { text-transform: revert-layer; } #weight { font-weight: revert-layer; } #style { font-style: revert-layer; } #break { word-break: revert-layer; } #fallback { text-transform: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let transform = node("<div id='transform'>Target</div>");
        let weight = node("<div id='weight'>Target</div>");
        let style = node("<div id='style'>Target</div>");
        let break_all = node("<div id='break'>Target</div>");
        let repeated = node("<div id='repeated'>Target</div>");
        let fallback = node("<div id='fallback'>Target</div>");

        assert_eq!(
            stylesheet.computed_for(&transform).text_transform(),
            TextTransformValue::Uppercase
        );
        assert_eq!(
            stylesheet.computed_for(&weight).font_weight(),
            FontWeightValue::Bold
        );
        assert_eq!(
            stylesheet.computed_for(&style).font_style(),
            FontStyleValue::Italic
        );
        assert_eq!(
            stylesheet.computed_for(&break_all).word_break(),
            WordBreakValue::BreakAll
        );
        assert_eq!(
            stylesheet.computed_for(&repeated).text_transform(),
            TextTransformValue::Lowercase
        );
        assert_eq!(
            stylesheet.computed_for(&fallback).text_transform(),
            TextTransformValue::None
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_inherited_text_spacing() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { word-spacing: 4px; letter-spacing: 5px; } #rollback { word-spacing: 6px; letter-spacing: 7px; } #repeated { word-spacing: 8px; letter-spacing: 9px; } #fallback { word-spacing: revert-layer; letter-spacing: revert-layer; } #invalid { word-spacing: 10px; letter-spacing: 11px; } } @layer theme { #named { word-spacing: 12px; letter-spacing: 13px; } #rollback { word-spacing: 14px; letter-spacing: 15px; } #repeated { word-spacing: revert-layer; letter-spacing: revert-layer; } #invalid { word-spacing: 16px; letter-spacing: 17px; } } @layer top { #repeated { word-spacing: revert-layer; letter-spacing: revert-layer; } } #named { word-spacing: revert-layer; letter-spacing: revert-layer; } #rollback { word-spacing: 18px; letter-spacing: revert-layer; } #fallback { word-spacing: revert-layer; letter-spacing: revert-layer; } #inline { word-spacing: 22px; letter-spacing: 23px; } #invalid { word-spacing: 20px; word-spacing: 1px 2px; letter-spacing: 21px; letter-spacing: normal; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let inline = node("<div id='inline'>Inline</div>");

        let assert_values = |element: &NativeNode, word: u32, letter: u32| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.word_spacing(), word);
            assert_eq!(style.letter_spacing(), letter);
        };

        assert_values(&named, 12, 13);
        assert_values(&rollback, 18, 15);
        assert_values(&repeated, 8, 9);
        assert_values(&fallback, 0, 0);
        assert_values(&invalid, 20, 21);
        assert_values(&inline, 22, 23);
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_inherited_vertical_align() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { vertical-align: top; } #rollback { vertical-align: middle; } #repeated { vertical-align: bottom; } #fallback { vertical-align: revert-layer; } #invalid { vertical-align: top; } } @layer theme { #named { vertical-align: bottom; } #rollback { vertical-align: top; } #repeated { vertical-align: revert-layer; } #invalid { vertical-align: middle; } } @layer top { #repeated { vertical-align: revert-layer; } } #named { vertical-align: revert-layer; } #rollback { vertical-align: middle; vertical-align: revert-layer; } #fallback { vertical-align: revert-layer; } #invalid { vertical-align: bottom; vertical-align: 1px; } #inline { vertical-align: top; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let fallback = node("<div id='fallback'>Fallback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let inline = node("<div id='inline'>Inline</div>");

        let assert_value = |element: &NativeNode, expected: VerticalAlignValue| {
            assert_eq!(stylesheet.computed_for(element).vertical_align(), expected);
        };
        assert_value(&named, VerticalAlignValue::Bottom);
        assert_value(&rollback, VerticalAlignValue::Top);
        assert_value(&repeated, VerticalAlignValue::Bottom);
        assert_value(&fallback, VerticalAlignValue::Baseline);
        assert_value(&invalid, VerticalAlignValue::Bottom);
        assert_value(&inline, VerticalAlignValue::Top);
    }

    #[test]
    fn text_decoration_parser_accepts_bounded_line_sets() {
        assert_eq!(
            parse_text_decoration("UNDERLINE"),
            Some(TextDecorationValue::new(true, false, false))
        );
        assert_eq!(
            parse_text_decoration("OVERLINE"),
            Some(TextDecorationValue::new(false, true, false))
        );
        assert_eq!(
            parse_text_decoration("line-through"),
            Some(TextDecorationValue::new(false, false, true))
        );
        assert_eq!(
            parse_text_decoration("none"),
            Some(TextDecorationValue::none())
        );
        assert_eq!(
            parse_text_decoration("underline overline line-through"),
            Some(TextDecorationValue::new(true, true, true))
        );
        assert_eq!(
            parse_text_decoration("LINE-THROUGH underline OVERLINE"),
            Some(TextDecorationValue::new(true, true, true))
        );
        assert_eq!(
            parse_text_decoration("underline overline"),
            Some(TextDecorationValue::new(true, true, false))
        );
        assert_eq!(parse_text_decoration("underline underline"), None);
        assert_eq!(parse_text_decoration("none underline"), None);
        assert_eq!(parse_text_decoration("underline none"), None);
        assert_eq!(parse_text_decoration("none none"), None);
        assert_eq!(parse_text_decoration("underline red"), None);
        assert_eq!(parse_text_decoration(""), None);
    }

    #[test]
    fn text_decoration_line_longhand_shares_ordered_line_state() {
        let shorthand_then_longhand = parse_declarations(
            "text-decoration: underline; text-decoration-line: overline line-through",
        );
        assert_eq!(
            shorthand_then_longhand.text_decoration,
            Some(NativeTextDecorationDeclaration::Value(
                TextDecorationValue::new(false, true, true)
            ))
        );

        let longhand_then_shorthand = parse_declarations(
            "text-decoration-line: overline line-through; text-decoration: underline",
        );
        assert_eq!(
            longhand_then_shorthand.text_decoration,
            Some(NativeTextDecorationDeclaration::Value(
                TextDecorationValue::new(true, false, false)
            ))
        );
        assert_eq!(
            parse_declarations("text-decoration-line: none").text_decoration,
            Some(NativeTextDecorationDeclaration::Value(
                TextDecorationValue::none()
            ))
        );
        assert_eq!(
            parse_declarations("text-decoration-line: underline underline").text_decoration,
            None
        );
    }

    #[test]
    fn text_decoration_style_parser_accepts_only_bounded_patterns() {
        assert_eq!(parse_border_style("SOLID"), Some(NativeBorderStyle::Solid));
        assert_eq!(
            parse_border_style("dashed"),
            Some(NativeBorderStyle::Dashed)
        );
        assert_eq!(
            parse_border_style("DOTTED"),
            Some(NativeBorderStyle::Dotted)
        );
        assert_eq!(
            parse_border_style("DOUBLE"),
            Some(NativeBorderStyle::Double)
        );
        assert_eq!(
            parse_border_style("groove"),
            Some(NativeBorderStyle::Groove)
        );
        assert_eq!(parse_border_style("RIDGE"), Some(NativeBorderStyle::Ridge));
        assert_eq!(parse_border_style("inset"), Some(NativeBorderStyle::Inset));
        assert_eq!(
            parse_border_style("OUTSET"),
            Some(NativeBorderStyle::Outset)
        );
        assert_eq!(parse_border_style("wavy"), None);
        assert_eq!(parse_border_style("solid dashed"), None);
        assert_eq!(parse_border_style(""), None);
        assert_eq!(
            parse_text_decoration_style("SOLID"),
            Some(NativeTextDecorationStyleDeclaration::Value(
                NativeTextDecorationStyle::Solid
            ))
        );
        assert_eq!(
            parse_text_decoration_style("dashed"),
            Some(NativeTextDecorationStyleDeclaration::Value(
                NativeTextDecorationStyle::Dashed
            ))
        );
        assert_eq!(
            parse_text_decoration_style("DOTTED"),
            Some(NativeTextDecorationStyleDeclaration::Value(
                NativeTextDecorationStyle::Dotted
            ))
        );
        assert_eq!(
            parse_text_decoration_style("DoUbLe"),
            Some(NativeTextDecorationStyleDeclaration::Value(
                NativeTextDecorationStyle::Double
            ))
        );
        assert_eq!(
            parse_text_decoration_style("WaVy"),
            Some(NativeTextDecorationStyleDeclaration::Value(
                NativeTextDecorationStyle::Wavy
            ))
        );
        assert_eq!(
            parse_text_decoration_style("ReVeRt-LaYeR"),
            Some(NativeTextDecorationStyleDeclaration::RevertLayer)
        );
        assert_eq!(parse_text_decoration_style("zigzag"), None);
        assert_eq!(parse_text_decoration_style("solid double"), None);
        assert_eq!(parse_text_decoration_style("revert"), None);
        assert_eq!(parse_text_decoration_style("inherit"), None);
        assert_eq!(parse_text_decoration_style(""), None);
    }

    #[test]
    fn text_decoration_style_is_inherited_and_cascaded() {
        let document = NativeDocument::parse(
            "<style>div { text-decoration-style: dotted; } #parent { text-decoration-style: dashed; } #explicit { text-decoration-style: solid; } #double { text-decoration-style: double; } #wavy { text-decoration-style: WaVy; } #invalid { text-decoration-style: zigzag; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='double'>Double</span><span id='wavy'>Wavy</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-style:dotted'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let inherited = document.resolve_target("id=inherited").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let double = document.resolve_target("id=double").unwrap();
        let wavy = document.resolve_target("id=wavy").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();

        assert_eq!(
            document
                .computed_style_for_layout(parent)
                .text_decoration_style(),
            NativeTextDecorationStyle::Dashed
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherited)
                .text_decoration_style(),
            NativeTextDecorationStyle::Dashed
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_decoration_style(),
            NativeTextDecorationStyle::Solid
        );
        assert_eq!(
            document
                .computed_style_for_layout(double)
                .text_decoration_style(),
            NativeTextDecorationStyle::Double
        );
        assert_eq!(
            document
                .computed_style_for_layout(wavy)
                .text_decoration_style(),
            NativeTextDecorationStyle::Wavy
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration_style(),
            NativeTextDecorationStyle::Dashed
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline)
                .text_decoration_style(),
            NativeTextDecorationStyle::Dotted
        );
    }

    #[test]
    fn text_decoration_skip_ink_parser_accepts_only_bounded_values() {
        assert_eq!(
            NativeInheritedStyle::default().text_decoration_skip_ink,
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(
            parse_text_decoration_skip_ink("AUTO"),
            Some(NativeTextDecorationSkipInkDeclaration::Value(
                NativeTextDecorationSkipInk::Auto
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_ink("NoNe"),
            Some(NativeTextDecorationSkipInkDeclaration::Value(
                NativeTextDecorationSkipInk::None
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_ink("ReVeRt-LaYeR"),
            Some(NativeTextDecorationSkipInkDeclaration::RevertLayer)
        );
        assert_eq!(parse_text_decoration_skip_ink("all"), None);
        assert_eq!(parse_text_decoration_skip_ink("inherit"), None);
        assert_eq!(parse_text_decoration_skip_ink("revert"), None);
        assert_eq!(parse_text_decoration_skip_ink(""), None);
        assert_eq!(
            parse_declarations("text-decoration-skip-ink: none").text_decoration_skip_ink,
            Some(NativeTextDecorationSkipInkDeclaration::Value(
                NativeTextDecorationSkipInk::None
            ))
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-ink: all").text_decoration_skip_ink,
            None
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-ink: revert-layer").text_decoration_skip_ink,
            Some(NativeTextDecorationSkipInkDeclaration::RevertLayer)
        );
    }

    #[test]
    fn text_decoration_skip_spaces_parser_accepts_bounded_edge_values() {
        assert_eq!(
            NativeInheritedStyle::default().text_decoration_skip_spaces,
            NativeTextDecorationSkipSpaces::None
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("NONE"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::None
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("AlL"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::All
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("start"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::Start
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("END"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::End
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("start end"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("END start"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("INITIAL"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd
            ))
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("InHeRiT"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("uNsEt"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Unset)
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("ReVeRt"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Revert)
        );
        assert_eq!(parse_text_decoration_skip_spaces("start start"), None);
        assert_eq!(parse_text_decoration_skip_spaces("none start"), None);
        assert_eq!(parse_text_decoration_skip_spaces("all end"), None);
        assert_eq!(
            parse_text_decoration_skip_spaces("revert"),
            Some(NativeTextDecorationSkipSpacesDeclaration::Revert)
        );
        assert_eq!(
            parse_text_decoration_skip_spaces("ReVeRt-LaYeR"),
            Some(NativeTextDecorationSkipSpacesDeclaration::RevertLayer)
        );
        assert_eq!(parse_text_decoration_skip_spaces(""), None);
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: all").text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::All
            ))
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: start end")
                .text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd
            ))
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: initial").text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::Value(
                NativeTextDecorationSkipSpaces::StartAndEnd
            ))
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: inherit").text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::Inherit)
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: unset").text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::Unset)
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: revert").text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::Revert)
        );
        assert_eq!(
            parse_declarations("text-decoration-skip-spaces: revert-layer")
                .text_decoration_skip_spaces,
            Some(NativeTextDecorationSkipSpacesDeclaration::RevertLayer)
        );
    }

    #[test]
    fn text_decoration_skip_spaces_is_inherited_and_cascaded() {
        let document = NativeDocument::parse(
            "<style>div { text-decoration-skip-spaces: none; } #parent { text-decoration-skip-spaces: START END; } #explicit { text-decoration-skip-spaces: end; } #inherit { text-decoration-skip-spaces: INHERIT; } #unset { text-decoration-skip-spaces: UNSET; } #revert { text-decoration-skip-spaces: ReVeRt; } #inline-inherit { text-decoration-skip-spaces: end; } #invalid { text-decoration-skip-spaces: start start; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='inline-inherit' style='text-decoration-skip-spaces:inherit'>Inline inherit</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-skip-spaces:start'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let inherited = document.resolve_target("id=inherited").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let inherit = document.resolve_target("id=inherit").unwrap();
        let unset = document.resolve_target("id=unset").unwrap();
        let revert = document.resolve_target("id=revert").unwrap();
        let inline_inherit = document.resolve_target("id=inline-inherit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::None
        );
        assert_eq!(
            document
                .computed_style_for_layout(parent)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherited)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::End
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherit)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(unset)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(revert)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline_inherit)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::StartAndEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline)
                .text_decoration_skip_spaces(),
            NativeTextDecorationSkipSpaces::Start
        );
    }

    #[test]
    fn text_decoration_skip_ink_is_inherited_and_cascaded() {
        let document = NativeDocument::parse(
            "<style>div { text-decoration-skip-ink: none; } #parent { text-decoration-skip-ink: AUTO; } #explicit { text-decoration-skip-ink: none; } #invalid { text-decoration-skip-ink: all; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-skip-ink:none'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let inherited = document.resolve_target("id=inherited").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(
            document
                .computed_style_for_layout(parent)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherited)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::None
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline)
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::None
        );
    }

    #[test]
    fn text_decoration_thickness_parser_accepts_only_bounded_pixels() {
        assert_eq!(NativeInheritedStyle::default().text_decoration_thickness, 1);
        assert_eq!(
            parse_text_decoration_thickness("1px"),
            Some(NativeTextDecorationThicknessDeclaration::Value(1))
        );
        assert_eq!(
            parse_text_decoration_thickness("2PX"),
            Some(NativeTextDecorationThicknessDeclaration::Value(2))
        );
        assert_eq!(
            parse_text_decoration_thickness("4px"),
            Some(NativeTextDecorationThicknessDeclaration::Value(
                MAX_NATIVE_TEXT_DECORATION_THICKNESS
            ))
        );
        assert_eq!(
            parse_text_decoration_thickness("ReVeRt-LaYeR"),
            Some(NativeTextDecorationThicknessDeclaration::RevertLayer)
        );
        assert_eq!(parse_text_decoration_thickness("0px"), None);
        assert_eq!(parse_text_decoration_thickness("5px"), None);
        assert_eq!(parse_text_decoration_thickness("auto"), None);
        assert_eq!(parse_text_decoration_thickness("1.5px"), None);
        assert_eq!(parse_text_decoration_thickness("2px 3px"), None);
        assert_eq!(parse_text_decoration_thickness("from-font"), None);
        assert_eq!(parse_text_decoration_thickness("revert"), None);
        assert_eq!(parse_text_decoration_thickness("inherit"), None);
        assert_eq!(parse_text_decoration_thickness("unset"), None);
        assert_eq!(parse_text_decoration_thickness("initial"), None);
        assert_eq!(parse_text_decoration_thickness("all"), None);
    }

    #[test]
    fn text_decoration_thickness_is_inherited_and_cascaded() {
        let document = NativeDocument::parse(
            "<style>div { text-decoration-thickness: 4px; } #parent { text-decoration-thickness: 3px; } #explicit { text-decoration-thickness: 1px; } #invalid { text-decoration-thickness: 5px; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-thickness:2px'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let inherited = document.resolve_target("id=inherited").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_decoration_thickness(),
            1
        );
        assert_eq!(
            document
                .computed_style_for_layout(parent)
                .text_decoration_thickness(),
            3
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherited)
                .text_decoration_thickness(),
            3
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_decoration_thickness(),
            1
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration_thickness(),
            3
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline)
                .text_decoration_thickness(),
            2
        );
    }

    #[test]
    fn text_underline_offset_parser_accepts_only_bounded_signed_pixels() {
        assert_eq!(NativeInheritedStyle::default().text_underline_offset, 0);
        assert_eq!(
            parse_text_underline_offset("-4px"),
            Some(NativeTextUnderlineOffsetDeclaration::Value(
                MIN_NATIVE_TEXT_UNDERLINE_OFFSET
            ))
        );
        assert_eq!(
            parse_text_underline_offset("-0PX"),
            Some(NativeTextUnderlineOffsetDeclaration::Value(0))
        );
        assert_eq!(
            parse_text_underline_offset("0px"),
            Some(NativeTextUnderlineOffsetDeclaration::Value(0))
        );
        assert_eq!(
            parse_text_underline_offset("4px"),
            Some(NativeTextUnderlineOffsetDeclaration::Value(4))
        );
        assert_eq!(
            parse_text_underline_offset("ReVeRt-LaYeR"),
            Some(NativeTextUnderlineOffsetDeclaration::RevertLayer)
        );
        assert_eq!(parse_text_underline_offset("-5px"), None);
        assert_eq!(parse_text_underline_offset("5px"), None);
        assert_eq!(parse_text_underline_offset("+1px"), None);
        assert_eq!(parse_text_underline_offset("1.5px"), None);
        assert_eq!(parse_text_underline_offset("auto"), None);
        assert_eq!(parse_text_underline_offset("10%"), None);
        assert_eq!(parse_text_underline_offset("2px 3px"), None);
    }

    #[test]
    fn text_underline_offset_is_inherited_and_cascaded() {
        let document = NativeDocument::parse(
            "<style>div { text-underline-offset: 4px; } #parent { text-underline-offset: -3px; } #explicit { text-underline-offset: 0px; } #invalid { text-underline-offset: 5px; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span><span id='inline' style='text-underline-offset:2px'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let inherited = document.resolve_target("id=inherited").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_underline_offset(),
            0
        );
        assert_eq!(
            document
                .computed_style_for_layout(parent)
                .text_underline_offset(),
            -3
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherited)
                .text_underline_offset(),
            -3
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_underline_offset(),
            0
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_underline_offset(),
            -3
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline)
                .text_underline_offset(),
            2
        );
    }

    #[test]
    fn text_decoration_color_is_local_and_uses_existing_color_values() {
        let blue = NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        };
        let transparent = NativeColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        };
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-decoration-color: red; } #target { text-decoration-color: blue; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target' style='text-decoration-color: #010203'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_decoration_color(),
            Some(NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: u8::MAX,
            })
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-decoration: underline; text-decoration-color: red; color: green; } #explicit { text-decoration-color: blue; } #transparent { text-decoration-color: transparent; } #invalid { text-decoration-color: currentColor; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='transparent'>Transparent</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let inherited = document.resolve_target("id=inherited").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let transparent_node = document.resolve_target("id=transparent").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document
                .computed_style_for_layout(parent)
                .text_decoration_color(),
            Some(NativeColor::RED)
        );
        assert_eq!(
            document
                .computed_style_for_layout(inherited)
                .text_decoration_color(),
            None
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_decoration_color(),
            Some(blue)
        );
        assert_eq!(
            document
                .computed_style_for_layout(transparent_node)
                .text_decoration_color(),
            Some(transparent)
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration_color(),
            None
        );
    }

    #[test]
    fn text_decoration_color_parser_accepts_only_bounded_values_and_revert_layer() {
        assert_eq!(
            parse_text_decoration_color("ReVeRt-LaYeR"),
            Some(NativeTextDecorationColorDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_decoration_color("blue"),
            Some(NativeTextDecorationColorDeclaration::Value(NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            }))
        );
        assert_eq!(
            parse_text_decoration_color("transparent"),
            Some(NativeTextDecorationColorDeclaration::Value(NativeColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            }))
        );
        assert_eq!(parse_text_decoration_color("currentColor"), None);
        assert_eq!(parse_text_decoration_color("inherit"), None);
        assert_eq!(parse_text_decoration_color("unset"), None);
        assert_eq!(parse_text_decoration_color("revert"), None);
        assert_eq!(
            parse_text_decoration_color("linear-gradient(red, blue)"),
            None
        );
        assert_eq!(
            parse_declarations("text-decoration-color: revert-layer").text_decoration_color,
            Some(NativeTextDecorationColorDeclaration::RevertLayer)
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_color_candidates() {
        let document = NativeDocument::parse(
            "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:green; text-decoration:underline; } @layer base { #named { text-decoration-color:red; } #rollback { text-decoration-color:red; } #repeated { text-decoration-color:red; } #unlayered { text-decoration-color:red; } #inline { text-decoration-color:red; } #fallback { text-decoration-color:revert-layer; } } @layer theme { .named { text-decoration-color:blue; } #rollback { text-decoration-color:blue; } #repeated { text-decoration-color:revert-layer; } #unlayered { text-decoration-color:blue; } #inline { text-decoration-color:blue; } } @layer top { #repeated { text-decoration-color:revert-layer; } } #named { text-decoration-color:revert-layer; } #unlayered { text-decoration-color:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-color:revert-layer'>A</div><div id='fallback' class='line'>A</div><div id='inherited' class='line' style='text-decoration-color:red'><span id='child'>A</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let blue = NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        };
        let color_for = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
                .text_decoration_color()
        };
        assert_eq!(color_for("named"), Some(blue));
        assert_eq!(color_for("rollback"), Some(blue));
        assert_eq!(color_for("repeated"), Some(NativeColor::RED));
        assert_eq!(color_for("unlayered"), Some(blue));
        assert_eq!(color_for("inline"), Some(blue));
        assert_eq!(color_for("fallback"), None);
        assert_eq!(color_for("child"), None);
    }

    #[test]
    fn text_decoration_declaration_parser_accepts_only_revert_layer_or_line_values() {
        assert_eq!(
            parse_text_decoration_declaration("ReVeRt-LaYeR"),
            Some(NativeTextDecorationDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_decoration_declaration("underline overline line-through"),
            Some(NativeTextDecorationDeclaration::Value(
                TextDecorationValue::new(true, true, true)
            ))
        );
        assert_eq!(
            parse_text_decoration_declaration("text-decoration-line: revert-layer"),
            None
        );
        for value in [
            "initial",
            "inherit",
            "unset",
            "revert",
            "all",
            "revert-layer underline",
            "underline revert-layer",
            "none underline",
            "underline underline",
            "",
        ] {
            assert_eq!(parse_text_decoration_declaration(value), None, "{value}");
        }
        assert_eq!(
            parse_declarations("text-decoration-line: revert-layer").text_decoration,
            Some(NativeTextDecorationDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_declarations("text-decoration: revert-layer").text_decoration,
            Some(NativeTextDecorationDeclaration::RevertLayer)
        );
    }

    #[test]
    fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_candidates() {
        let document = NativeDocument::parse(
            "<style>.line { display:block; width:24px; height:20px; line-height:20px; } @layer base { #named { text-decoration-line:underline; } #rollback { text-decoration:underline; } #repeated { text-decoration:underline; } #unlayered { text-decoration:underline; } #inline { text-decoration:underline; } #fallback { text-decoration-line:revert-layer; } } @layer theme { .named { text-decoration-line:overline; } #rollback { text-decoration-line:revert-layer; } #repeated { text-decoration-line:revert-layer; } #unlayered { text-decoration-line:overline; } #inline { text-decoration-line:overline; } } @layer top { #repeated { text-decoration-line:revert-layer; } } #named { text-decoration-line:revert-layer; } #unlayered { text-decoration-line:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-line:revert-layer'>A</div><div id='fallback' class='line'>A</div><div id='parent' class='line' style='text-decoration:line-through'><span id='child' style='text-decoration-line:revert-layer'>A</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let line_for = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
                .text_decoration()
        };
        assert_eq!(
            line_for("named"),
            TextDecorationValue::new(false, true, false)
        );
        assert_eq!(
            line_for("rollback"),
            TextDecorationValue::new(true, false, false)
        );
        assert_eq!(
            line_for("repeated"),
            TextDecorationValue::new(true, false, false)
        );
        assert_eq!(
            line_for("unlayered"),
            TextDecorationValue::new(false, true, false)
        );
        assert_eq!(
            line_for("inline"),
            TextDecorationValue::new(false, true, false)
        );
        assert_eq!(line_for("fallback"), TextDecorationValue::none());
        assert_eq!(
            line_for("child"),
            TextDecorationValue::new(false, false, true)
        );
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
    fn inherited_text_declaration_parsers_accept_only_standalone_revert_layer() {
        assert_eq!(
            parse_text_transform_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_font_weight_declaration("revert-layer"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_font_style_declaration(" REVERT-LAYER "),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_word_break_declaration("revert-layer"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_transform_declaration("revert-layer uppercase"),
            None
        );
        assert_eq!(parse_font_weight_declaration("revert"), None);
        assert_eq!(parse_font_style_declaration("oblique"), None);
        assert_eq!(parse_word_break_declaration("keep-all"), None);
        assert_eq!(
            parse_text_transform_declaration("uppercase"),
            Some(InheritedTextDeclaration::Value(
                TextTransformValue::Uppercase
            ))
        );
    }

    #[test]
    fn inherited_text_spacing_declaration_parsers_accept_only_standalone_revert_layer() {
        assert_eq!(
            parse_word_spacing_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_letter_spacing_declaration(" REVERT-LAYER "),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_word_spacing_declaration("16px"),
            Some(InheritedTextDeclaration::Value(16))
        );
        assert_eq!(parse_letter_spacing_declaration("revert-layer 2px"), None);
        assert_eq!(parse_word_spacing_declaration("revert"), None);
        assert_eq!(parse_letter_spacing_declaration("normal"), None);
        assert_eq!(parse_word_spacing_declaration("-1px"), None);
        assert_eq!(parse_letter_spacing_declaration("1.5px"), None);
        assert_eq!(parse_word_spacing_declaration("2em"), None);
    }

    #[test]
    fn inherited_text_declarations_preserve_valid_values_before_invalid_later_values() {
        let declarations = parse_declarations(
            "text-transform: uppercase; text-transform: capitalize; font-weight: bold; font-weight: 500; font-style: italic; font-style: oblique; word-break: break-all; word-break: keep-all; word-spacing: 12px; word-spacing: 1px 2px; letter-spacing: 13px; letter-spacing: normal;",
        );
        assert_eq!(
            declarations.text_transform,
            Some(InheritedTextDeclaration::Value(
                TextTransformValue::Uppercase
            ))
        );
        assert_eq!(
            declarations.font_weight,
            Some(InheritedTextDeclaration::Value(FontWeightValue::Bold))
        );
        assert_eq!(
            declarations.font_style,
            Some(InheritedTextDeclaration::Value(FontStyleValue::Italic))
        );
        assert_eq!(
            declarations.word_break,
            Some(InheritedTextDeclaration::Value(WordBreakValue::BreakAll))
        );
        assert_eq!(
            declarations.word_spacing,
            Some(InheritedTextDeclaration::Value(12))
        );
        assert_eq!(
            declarations.letter_spacing,
            Some(InheritedTextDeclaration::Value(13))
        );
    }

    #[test]
    fn inherited_vertical_align_declaration_parser_accepts_only_standalone_revert_layer() {
        assert_eq!(
            parse_vertical_align_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_vertical_align_declaration("middle"),
            Some(InheritedTextDeclaration::Value(VerticalAlignValue::Middle))
        );
        assert_eq!(
            parse_vertical_align_declaration("revert-layer middle"),
            None
        );
        assert_eq!(parse_vertical_align_declaration("1px"), None);
        assert_eq!(parse_vertical_align_declaration("50%"), None);
        assert_eq!(parse_vertical_align_declaration("text-top"), None);
        let declarations = parse_declarations("vertical-align: bottom; vertical-align: 1px;");
        assert_eq!(
            declarations.vertical_align,
            Some(InheritedTextDeclaration::Value(VerticalAlignValue::Bottom))
        );
    }

    #[test]
    fn local_text_declaration_parsers_accept_only_standalone_revert_layer() {
        assert_eq!(
            parse_text_overflow_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_overflow_declaration("ellipsis"),
            Some(LocalCascadeDeclaration::Value(TextOverflowValue::Ellipsis))
        );
        assert_eq!(
            parse_text_indent_declaration(" REVERT-LAYER "),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_indent_declaration("16px"),
            Some(LocalCascadeDeclaration::Value(16))
        );
        assert_eq!(
            parse_text_overflow_declaration("revert-layer ellipsis"),
            None
        );
        assert_eq!(parse_text_indent_declaration("revert"), None);
        assert_eq!(parse_text_indent_declaration("-1px"), None);
        assert_eq!(parse_text_indent_declaration("50%"), None);
        let declarations = parse_declarations(
            "text-indent: 12px; text-indent: 1px 2px; text-overflow: ellipsis; text-overflow: fade;",
        );
        assert_eq!(
            declarations.text_indent,
            Some(LocalCascadeDeclaration::Value(12))
        );
        assert_eq!(
            declarations.text_overflow,
            Some(LocalCascadeDeclaration::Value(TextOverflowValue::Ellipsis))
        );
    }

    #[test]
    fn local_dimension_declaration_parser_accepts_only_standalone_revert_layer() {
        assert_eq!(
            parse_local_dimension_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_local_dimension_declaration("24px"),
            Some(LocalCascadeDeclaration::Value(24))
        );
        assert_eq!(parse_local_dimension_declaration("revert-layer 24px"), None);
        assert_eq!(parse_local_dimension_declaration("-1px"), None);
        assert_eq!(parse_local_dimension_declaration("50%"), None);
        assert_eq!(parse_local_dimension_declaration("auto"), None);
        let declarations = parse_declarations(
            "width:24px;width:1px 2px;height:30px;height:bad;min-width:8px;min-width:-1px;max-height:40px;max-height:50%;",
        );
        assert_eq!(declarations.width, Some(LocalCascadeDeclaration::Value(24)));
        assert_eq!(
            declarations.height,
            Some(LocalCascadeDeclaration::Value(30))
        );
        assert_eq!(
            declarations.min_width,
            Some(LocalCascadeDeclaration::Value(8))
        );
        assert_eq!(
            declarations.max_height,
            Some(LocalCascadeDeclaration::Value(40))
        );
    }

    #[test]
    fn local_box_model_declaration_parsers_accept_only_standalone_revert_layer() {
        assert_eq!(
            parse_local_box_edges(" ReVeRt-LaYeR "),
            Some([LocalCascadeDeclaration::RevertLayer; 4])
        );
        assert_eq!(
            parse_local_padding_declaration("4px"),
            Some(LocalCascadeDeclaration::Value(4))
        );
        assert_eq!(
            parse_local_padding_declaration("revert-layer"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(parse_local_box_edges("revert-layer 4px"), None);
        assert_eq!(parse_local_box_edges("50%"), None);
        assert_eq!(parse_local_box_edges("-1px"), None);
        assert_eq!(parse_local_box_edges("1px 2px 3px 4px 5px"), None);
        assert_eq!(
            parse_local_margin_edges("revert-layer"),
            Some([LocalCascadeDeclaration::RevertLayer; 4])
        );
        assert_eq!(
            parse_local_margin_declaration("auto"),
            Some(LocalCascadeDeclaration::Value(NativeMarginValue::Auto))
        );
        assert_eq!(
            parse_local_margin_declaration("revert-layer"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(parse_local_margin_edges("revert-layer auto"), None);
        assert_eq!(parse_local_margin_edges("50%"), None);
        assert_eq!(parse_local_margin_edges("-1px"), None);
        assert_eq!(
            parse_local_box_sizing_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_local_box_sizing_declaration("border-box"),
            Some(LocalCascadeDeclaration::Value(NativeBoxSizing::BorderBox))
        );
        assert_eq!(
            parse_local_box_sizing_declaration("revert-layer border-box"),
            None
        );
        assert_eq!(parse_local_box_sizing_declaration("auto"), None);

        let declarations = parse_declarations(
            "padding: 1px 2px 3px 4px; padding-left: revert-layer; padding-right: 6px; padding-right: revert-layer; margin: auto 2px; margin-bottom: revert-layer; box-sizing: border-box; box-sizing: invalid;",
        );
        assert_eq!(
            declarations.padding,
            [
                Some(LocalCascadeDeclaration::Value(1)),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(3)),
                Some(LocalCascadeDeclaration::RevertLayer),
            ]
        );
        assert_eq!(
            declarations.margin,
            [
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Auto)),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(2))),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(2))),
            ]
        );
        assert_eq!(
            declarations.box_sizing,
            Some(LocalCascadeDeclaration::Value(NativeBoxSizing::BorderBox))
        );
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
            "<style>#parent { text-align: center; } #explicit { text-align: right; } #justified { text-align: justify; } #invalid { text-align: match-parent; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='justified'>Justified</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let justified = document.resolve_target("id=justified").unwrap();
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
            document.computed_style_for_layout(justified).text_align(),
            TextAlignValue::Justify
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).text_align(),
            TextAlignValue::Center
        );
    }

    #[test]
    fn text_align_last_is_cascaded_and_inherited_with_child_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-align-last: left; } #target { text-align-last: right; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target' style='text-align-last: center'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_align_last(),
            TextAlignLastValue::Center
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-align-last: center; } #explicit { text-align-last: end; } #justify { text-align-last: justify; } #invalid { text-align-last: match-parent; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='justify'>Justify</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let justify = document.resolve_target("id=justify").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_align_last(),
            TextAlignLastValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_align_last(),
            TextAlignLastValue::Center
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .text_align_last(),
            TextAlignLastValue::End
        );
        assert_eq!(
            document
                .computed_style_for_layout(justify)
                .text_align_last(),
            TextAlignLastValue::Justify
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_align_last(),
            TextAlignLastValue::Center
        );
    }

    #[test]
    fn text_justify_is_cascaded_and_inherited_with_child_precedence() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-justify: none; } #target { text-justify: inter-word; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target' style='text-justify: auto'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_justify(),
            TextJustifyValue::Auto
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-justify: none; } #explicit { text-justify: inter-word; } #auto { text-justify: auto; } #invalid { text-justify: inter-character; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='auto'>Auto</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let auto = document.resolve_target("id=auto").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_justify(),
            TextJustifyValue::None
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_justify(),
            TextJustifyValue::None
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).text_justify(),
            TextJustifyValue::InterWord
        );
        assert_eq!(
            document.computed_style_for_layout(auto).text_justify(),
            TextJustifyValue::Auto
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).text_justify(),
            TextJustifyValue::None
        );
    }

    #[test]
    fn white_space_revert_layer_rolls_back_named_and_inline_candidates() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { white-space: pre; } #repeated { white-space: pre-line; } #inline { white-space: nowrap; } } @layer theme { #named { white-space: pre-wrap; } #repeated { white-space: revert-layer; } #inline { white-space: pre; } } @layer top { #repeated { white-space: revert-layer; } } #named { white-space: revert-layer; } #repeated { white-space: revert-layer; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named'>Named</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let inline = node("<div id='inline' style='white-space:REVERT-LAYER'>Inline</div>");
        let fallback = node("<div id='fallback' style='white-space:ReVeRt-LaYeR'>Fallback</div>");

        assert_eq!(
            stylesheet.computed_for(&named).white_space(),
            WhiteSpaceValue::PreWrap
        );
        assert_eq!(
            stylesheet.computed_for(&repeated).white_space(),
            WhiteSpaceValue::PreLine
        );
        assert_eq!(
            stylesheet.computed_for(&inline).white_space(),
            WhiteSpaceValue::Pre
        );
        assert_eq!(
            stylesheet.computed_for(&fallback).white_space(),
            WhiteSpaceValue::Normal
        );

        let mut diagnostics = NativeDiagnosticSink::default();
        NativeStylesheet::from_sources_with_diagnostics(
            vec!["#target { white-space: revert-layer; }".into()],
            &mut diagnostics,
        )
        .unwrap();
        let (diagnostics, truncated) = diagnostics.finish();
        assert!(!truncated);
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "white-space"
        }));
    }

    #[test]
    fn logical_text_align_is_inherited_and_respects_direction_overrides() {
        let document = NativeDocument::parse(
            "<style>#parent { direction: rtl; text-align: start; } #ltr { direction: ltr; } #end { text-align: end; } #physical { text-align: right; } #invalid { text-align: match-parent; }</style><div id='parent'><span id='child'>Child</span><span id='ltr'>Ltr</span><span id='end'>End</span><span id='physical'>Physical</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let ltr = document.resolve_target("id=ltr").unwrap();
        let end = document.resolve_target("id=end").unwrap();
        let physical = document.resolve_target("id=physical").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_align(),
            TextAlignValue::Start
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_align(),
            TextAlignValue::Start
        );
        assert_eq!(
            document.computed_style_for_layout(ltr).direction(),
            DirectionValue::Ltr
        );
        assert_eq!(
            document.computed_style_for_layout(ltr).text_align(),
            TextAlignValue::Start
        );
        assert_eq!(
            document.computed_style_for_layout(end).text_align(),
            TextAlignValue::End
        );
        assert_eq!(
            document.computed_style_for_layout(physical).text_align(),
            TextAlignValue::Right
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).text_align(),
            TextAlignValue::Start
        );
    }

    #[test]
    fn text_decoration_is_inherited_and_child_none_clears_it() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "div { text-decoration: none; } #target { text-decoration: line-through; }".into(),
        ])
        .unwrap();
        let node = node("<div id='target'>Target</div>");
        assert_eq!(
            stylesheet.computed_for(&node).text_decoration(),
            TextDecorationValue::new(false, false, true)
        );

        let document = NativeDocument::parse(
            "<style>#parent { text-decoration: underline; } #clear { text-decoration: none; } #over { text-decoration: overline; } #through { text-decoration: line-through; } #longhand { text-decoration-line: overline line-through; } #invalid { text-decoration: blink; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='over'>Over</span><span id='through'>Through</span><span id='longhand'>Longhand</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let clear = document.resolve_target("id=clear").unwrap();
        let over = document.resolve_target("id=over").unwrap();
        let through = document.resolve_target("id=through").unwrap();
        let longhand = document.resolve_target("id=longhand").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).text_decoration(),
            TextDecorationValue::new(true, false, false)
        );
        assert_eq!(
            document.computed_style_for_layout(child).text_decoration(),
            TextDecorationValue::new(true, false, false)
        );
        assert_eq!(
            document.computed_style_for_layout(clear).text_decoration(),
            TextDecorationValue::none()
        );
        assert_eq!(
            document.computed_style_for_layout(over).text_decoration(),
            TextDecorationValue::new(false, true, false)
        );
        assert_eq!(
            document
                .computed_style_for_layout(through)
                .text_decoration(),
            TextDecorationValue::new(false, false, true)
        );
        assert_eq!(
            document
                .computed_style_for_layout(longhand)
                .text_decoration(),
            TextDecorationValue::new(false, true, true)
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .text_decoration(),
            TextDecorationValue::new(true, false, false)
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
    fn local_text_declarations_revert_layer_resolve_independently() {
        let document = NativeDocument::parse(
            "<style>@layer base { #named { text-indent:8px; text-overflow:clip; } #repeat { text-indent:4px; text-overflow:ellipsis; } #inline { text-indent:12px; text-overflow:ellipsis; } #fallback { text-indent:revert-layer; text-overflow:revert-layer; } #invalid { text-indent:16px; text-overflow:ellipsis; } } @layer theme { #named { text-indent:20px; text-overflow:ellipsis; } #repeat { text-indent:revert-layer; text-overflow:revert-layer; } #inline { text-indent:16px; text-overflow:clip; } } @layer top { #repeat { text-indent:revert-layer; text-overflow:revert-layer; } } #named { text-indent:revert-layer; text-overflow:revert-layer; } #repeat { text-indent:revert-layer; text-overflow:revert-layer; } #invalid { text-indent:1px 2px; text-overflow:fade; }</style><div id='named'>Named</div><div id='repeat'>Repeat</div><div id='inline' style='text-indent:revert-layer;text-overflow:revert-layer'>Inline</div><div id='fallback'>Fallback</div><div id='invalid'>Invalid</div><div id='parent' style='text-indent:20px;text-overflow:ellipsis'><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let named = document.resolve_target("id=named").unwrap();
        let repeat = document.resolve_target("id=repeat").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();
        let fallback = document.resolve_target("id=fallback").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();

        let style = |node| document.computed_style_for_layout(node);
        assert_eq!(style(named).text_indent(), 20);
        assert_eq!(style(named).text_overflow(), TextOverflowValue::Ellipsis);
        assert_eq!(style(repeat).text_indent(), 4);
        assert_eq!(style(repeat).text_overflow(), TextOverflowValue::Ellipsis);
        assert_eq!(style(inline).text_indent(), 16);
        assert_eq!(style(inline).text_overflow(), TextOverflowValue::Clip);
        assert_eq!(style(fallback).text_indent(), 0);
        assert_eq!(style(fallback).text_overflow(), TextOverflowValue::Clip);
        assert_eq!(style(invalid).text_indent(), 16);
        assert_eq!(style(invalid).text_overflow(), TextOverflowValue::Ellipsis);
        assert_eq!(style(parent).text_indent(), 20);
        assert_eq!(style(parent).text_overflow(), TextOverflowValue::Ellipsis);
        assert_eq!(style(child).text_indent(), 0);
        assert_eq!(style(child).text_overflow(), TextOverflowValue::Clip);
    }

    #[test]
    fn local_dimension_declarations_revert_layer_resolve_independently() {
        let document = NativeDocument::parse(
            "<style>@layer base { #named { width:24px; height:12px; min-width:4px; max-width:80px; min-height:6px; max-height:90px; } #repeat { width:20px; height:10px; min-width:2px; max-width:70px; min-height:4px; max-height:60px; } #fallback { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #invalid { width:30px; height:18px; min-width:8px; max-width:70px; min-height:10px; max-height:40px; } } @layer theme { #named { width:48px; height:20px; min-width:16px; max-width:100px; min-height:10px; max-height:110px; } #repeat { width:36px; height:16px; min-width:12px; max-width:90px; min-height:8px; max-height:80px; } #inline { width:32px; height:22px; min-width:14px; max-width:92px; min-height:12px; max-height:82px; } } @layer top { #repeat { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } } #named { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #repeat { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #invalid { width:1px 2px; height:1px 2px; min-width:1px 2px; max-width:1px 2px; min-height:1px 2px; max-height:50%; }</style><div id='named'>Named</div><div id='repeat'>Repeat</div><div id='inline' style='width:revert-layer;height:revert-layer;min-width:revert-layer;max-width:revert-layer;min-height:revert-layer;max-height:revert-layer'>Inline</div><div id='fallback'>Fallback</div><div id='invalid'>Invalid</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let named = document.resolve_target("id=named").unwrap();
        let repeat = document.resolve_target("id=repeat").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();
        let fallback = document.resolve_target("id=fallback").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        let style = |node| document.computed_style_for_layout(node);
        let assert_dimensions = |node, expected: [Option<u32>; 6]| {
            let computed = style(node);
            assert_eq!(
                [
                    computed.width(),
                    computed.height(),
                    computed.min_width(),
                    computed.max_width(),
                    computed.min_height(),
                    computed.max_height(),
                ],
                expected
            );
        };
        assert_dimensions(
            named,
            [Some(48), Some(20), Some(16), Some(100), Some(10), Some(110)],
        );
        assert_dimensions(
            repeat,
            [Some(36), Some(16), Some(12), Some(90), Some(8), Some(80)],
        );
        assert_dimensions(
            inline,
            [Some(32), Some(22), Some(14), Some(92), Some(12), Some(82)],
        );
        assert_dimensions(fallback, [None, None, None, None, None, None]);
        assert_dimensions(
            invalid,
            [Some(30), Some(18), Some(8), Some(70), Some(10), Some(40)],
        );
    }

    #[test]
    fn local_box_model_declarations_revert_layer_resolve_independently() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { box-sizing: content-box; padding: 1px 2px 3px 4px; margin: 1px 2px 3px 4px; } #repeated { box-sizing: border-box; padding: 2px; margin: auto 2px 3px 4px; } #fallback { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } #invalid { box-sizing: border-box; padding: 4px; margin: 5px; } } @layer theme { #named { box-sizing: border-box; padding: 5px 6px 7px 8px; margin: 5px 6px 7px 8px; } #repeated { box-sizing: content-box; padding: 6px; margin: revert-layer; } #inline { box-sizing: border-box; padding: 7px; margin: 8px; } } @layer top { #repeated { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } } #named { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } #repeated { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } #invalid { box-sizing: auto; padding: 1px 2px 3px 4px 5px; margin: -1px; }"
                .into(),
        ])
        .unwrap();
        let named = node("<div id='named' class='named'>Named</div>");
        let repeated = node("<div id='repeated'>Repeated</div>");
        let inline = node(
            "<div id='inline' style='box-sizing:revert-layer;padding:revert-layer;margin:revert-layer'>Inline</div>",
        );
        let fallback = node("<div id='fallback'>Fallback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");

        let named_style = stylesheet.computed_for(&named);
        assert!(named_style.is_border_box());
        assert_eq!(
            named_style.padding(),
            NativeBoxEdges {
                top: 5,
                right: 6,
                bottom: 7,
                left: 8,
            }
        );
        assert_eq!(
            named_style.margin(),
            NativeBoxEdges {
                top: 5,
                right: 6,
                bottom: 7,
                left: 8,
            }
        );

        let repeated_style = stylesheet.computed_for(&repeated);
        assert!(!repeated_style.is_border_box());
        assert_eq!(
            repeated_style.padding(),
            NativeBoxEdges {
                top: 6,
                right: 6,
                bottom: 6,
                left: 6,
            }
        );
        assert_eq!(
            repeated_style.margin(),
            NativeBoxEdges {
                top: 0,
                right: 2,
                bottom: 3,
                left: 4,
            }
        );
        assert!(repeated_style.margin_auto().top());
        assert!(!repeated_style.margin_auto().right());
        assert!(!repeated_style.margin_auto().bottom());
        assert!(!repeated_style.margin_auto().left());

        let inline_style = stylesheet.computed_for(&inline);
        assert!(inline_style.is_border_box());
        assert_eq!(
            inline_style.padding(),
            NativeBoxEdges::from_values([Some(7); 4])
        );
        assert_eq!(
            inline_style.margin(),
            NativeBoxEdges::from_values([Some(8); 4])
        );

        let fallback_style = stylesheet.computed_for(&fallback);
        assert!(!fallback_style.is_border_box());
        assert_eq!(fallback_style.padding(), NativeBoxEdges::default());
        assert_eq!(fallback_style.margin(), NativeBoxEdges::default());
        assert_eq!(fallback_style.margin_auto(), NativeAutoEdges::default());

        let invalid_style = stylesheet.computed_for(&invalid);
        assert!(invalid_style.is_border_box());
        assert_eq!(
            invalid_style.padding(),
            NativeBoxEdges::from_values([Some(4); 4])
        );
        assert_eq!(
            invalid_style.margin(),
            NativeBoxEdges::from_values([Some(5); 4])
        );
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
            "<style>#parent { gap: 16px 20px; } #explicit { gap: 24px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='gap: 32px'>Explicit</span><span id='invalid' style='gap: -1px'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(document.computed_style_for_layout(parent).column_gap(), 20);
        assert_eq!(document.computed_style_for_layout(parent).row_gap(), 16);
        assert_eq!(document.computed_style_for_layout(child).column_gap(), 0);
        assert_eq!(document.computed_style_for_layout(child).row_gap(), 0);
        assert_eq!(
            document.computed_style_for_layout(explicit).column_gap(),
            32
        );
        assert_eq!(document.computed_style_for_layout(explicit).row_gap(), 32);
        assert_eq!(document.computed_style_for_layout(invalid).column_gap(), 0);
        assert_eq!(document.computed_style_for_layout(invalid).row_gap(), 0);
    }

    #[test]
    fn row_gap_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { row-gap: 16px; } #explicit { row-gap: 24px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='row-gap: 32px'>Explicit</span><span id='invalid' style='row-gap: -1px'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(document.computed_style_for_layout(parent).row_gap(), 16);
        assert_eq!(document.computed_style_for_layout(child).row_gap(), 0);
        assert_eq!(document.computed_style_for_layout(explicit).row_gap(), 32);
        assert_eq!(document.computed_style_for_layout(invalid).row_gap(), 0);
    }

    #[test]
    fn gap_longhands_follow_declaration_order_and_inline_precedence() {
        let document = NativeDocument::parse(
            "<style>#later-row { row-gap: 2px; gap: 4px 6px; row-gap: 8px; column-gap: 10px; } #earlier-row { gap: 4px 6px; row-gap: 8px; column-gap: 10px; } .specificity { gap: 2px 3px; row-gap: 9px; } #specificity { row-gap: 7px; column-gap: 11px; }</style><div id='later-row'>Later</div><div id='earlier-row' style='row-gap: 12px; gap: 14px 16px; column-gap: 18px'>Earlier</div><div id='specificity' class='specificity'>Specificity</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let later_row = document.resolve_target("id=later-row").unwrap();
        let earlier_row = document.resolve_target("id=earlier-row").unwrap();
        let specificity = document.resolve_target("id=specificity").unwrap();

        let later_style = document.computed_style_for_layout(later_row);
        assert_eq!(later_style.column_gap(), 10);
        assert_eq!(later_style.row_gap(), 8);

        let earlier_style = document.computed_style_for_layout(earlier_row);
        assert_eq!(earlier_style.column_gap(), 18);
        assert_eq!(earlier_style.row_gap(), 14);

        let specificity_style = document.computed_style_for_layout(specificity);
        assert_eq!(specificity_style.column_gap(), 11);
        assert_eq!(specificity_style.row_gap(), 7);
    }

    #[test]
    fn flex_grow_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>.grow { flex-grow: 1; } #grow { flex-grow: 2; } #later { flex-grow: 3; flex-grow: 4; } #invalid { flex-grow: 2; flex-grow: 1.5; } #parent { flex-grow: 9; }</style><div id='parent'><span id='grow' class='grow' style='flex-grow: 3'>Grow</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let grow = document.resolve_target("id=grow").unwrap();
        let later = document.resolve_target("id=later").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let child = document.resolve_target("id=child").unwrap();

        assert_eq!(document.computed_style_for_layout(parent).flex_grow(), 9);
        assert_eq!(document.computed_style_for_layout(grow).flex_grow(), 3);
        assert_eq!(document.computed_style_for_layout(later).flex_grow(), 4);
        assert_eq!(document.computed_style_for_layout(invalid).flex_grow(), 2);
        assert_eq!(document.computed_style_for_layout(child).flex_grow(), 0);
    }

    #[test]
    fn flex_shrink_is_cascaded_without_inheriting_and_defaults_to_one() {
        let document = NativeDocument::parse(
            "<style>.shrink { flex-shrink: 1; } #shrink { flex-shrink: 2; } #later { flex-shrink: 3; flex-shrink: 4; } #invalid { flex-shrink: 2; flex-shrink: 1.5; } #parent { flex-shrink: 0; }</style><div id='parent'><span id='shrink' class='shrink' style='flex-shrink: 3'>Shrink</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let shrink = document.resolve_target("id=shrink").unwrap();
        let later = document.resolve_target("id=later").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let child = document.resolve_target("id=child").unwrap();

        assert_eq!(document.computed_style_for_layout(parent).flex_shrink(), 0);
        assert_eq!(document.computed_style_for_layout(shrink).flex_shrink(), 3);
        assert_eq!(document.computed_style_for_layout(later).flex_shrink(), 4);
        assert_eq!(document.computed_style_for_layout(invalid).flex_shrink(), 2);
        assert_eq!(document.computed_style_for_layout(child).flex_shrink(), 1);
    }

    #[test]
    fn flex_basis_is_cascaded_without_inheriting_and_defaults_to_auto() {
        let document = NativeDocument::parse(
            "<style>.basis { flex-basis: 12px; } #basis { flex-basis: 20px; } #later { flex-basis: 24px; flex-basis: 28px; } #invalid { flex-basis: 18px; flex-basis: 1.5px; } #parent { flex-basis: 40px; }</style><div id='parent'><span id='basis' class='basis' style='flex-basis: 16px'>Basis</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let basis = document.resolve_target("id=basis").unwrap();
        let later = document.resolve_target("id=later").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let child = document.resolve_target("id=child").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).flex_basis(),
            FlexBasisValue::Length(40)
        );
        assert_eq!(
            document.computed_style_for_layout(basis).flex_basis(),
            FlexBasisValue::Length(16)
        );
        assert_eq!(
            document.computed_style_for_layout(later).flex_basis(),
            FlexBasisValue::Length(28)
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).flex_basis(),
            FlexBasisValue::Length(18)
        );
        assert_eq!(
            document.computed_style_for_layout(child).flex_basis(),
            FlexBasisValue::Auto
        );
    }

    #[test]
    fn flex_shorthand_is_cascaded_without_inheriting_and_longhands_override_components() {
        let document = NativeDocument::parse(
            "<style>.preset { flex: 2 3 10px; } #preset { flex-grow: 4; } #later { flex: 1 0 8px; flex-shrink: 2; } #invalid { flex: 2 0 6px; flex: 1 2 3%; } #parent { flex: none; }</style><div id='parent'><span id='preset' class='preset' style='flex-basis: 16px'>Preset</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let preset = document.resolve_target("id=preset").unwrap();
        let later = document.resolve_target("id=later").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let child = document.resolve_target("id=child").unwrap();

        let parent_style = document.computed_style_for_layout(parent);
        assert_eq!(parent_style.flex_grow(), 0);
        assert_eq!(parent_style.flex_shrink(), 0);
        assert_eq!(parent_style.flex_basis(), FlexBasisValue::Auto);

        let preset_style = document.computed_style_for_layout(preset);
        assert_eq!(preset_style.flex_grow(), 4);
        assert_eq!(preset_style.flex_shrink(), 3);
        assert_eq!(preset_style.flex_basis(), FlexBasisValue::Length(16));

        let later_style = document.computed_style_for_layout(later);
        assert_eq!(later_style.flex_grow(), 1);
        assert_eq!(later_style.flex_shrink(), 2);
        assert_eq!(later_style.flex_basis(), FlexBasisValue::Length(8));

        let invalid_style = document.computed_style_for_layout(invalid);
        assert_eq!(invalid_style.flex_grow(), 2);
        assert_eq!(invalid_style.flex_shrink(), 0);
        assert_eq!(invalid_style.flex_basis(), FlexBasisValue::Length(6));

        let child_style = document.computed_style_for_layout(child);
        assert_eq!(child_style.flex_grow(), 0);
        assert_eq!(child_style.flex_shrink(), 1);
        assert_eq!(child_style.flex_basis(), FlexBasisValue::Auto);
    }

    #[test]
    fn flex_flow_is_cascaded_without_inheriting_and_longhands_override_components() {
        let document = NativeDocument::parse(
            "<style>.flow { flex-flow: row-reverse wrap; } #flow { flex-wrap: nowrap; } #either { flex-flow: wrap-reverse row; } #column { flex-flow: column wrap; } #invalid { flex-flow: row-reverse wrap; flex-flow: column wrap wrap; } #parent { flex-flow: wrap-reverse; }</style><div id='parent'><span id='flow' class='flow'>Flow</span><span id='either'>Either</span><span id='column'>Column</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let flow = document.resolve_target("id=flow").unwrap();
        let either = document.resolve_target("id=either").unwrap();
        let column = document.resolve_target("id=column").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let child = document.resolve_target("id=child").unwrap();

        let parent_style = document.computed_style_for_layout(parent);
        assert_eq!(parent_style.flex_direction(), FlexDirectionValue::Row);
        assert_eq!(parent_style.flex_wrap(), FlexWrapValue::WrapReverse);

        let flow_style = document.computed_style_for_layout(flow);
        assert_eq!(flow_style.flex_direction(), FlexDirectionValue::RowReverse);
        assert_eq!(flow_style.flex_wrap(), FlexWrapValue::NoWrap);

        let either_style = document.computed_style_for_layout(either);
        assert_eq!(either_style.flex_direction(), FlexDirectionValue::Row);
        assert_eq!(either_style.flex_wrap(), FlexWrapValue::WrapReverse);

        let column_style = document.computed_style_for_layout(column);
        assert_eq!(column_style.flex_direction(), FlexDirectionValue::Column);
        assert_eq!(column_style.flex_wrap(), FlexWrapValue::Wrap);

        let invalid_style = document.computed_style_for_layout(invalid);
        assert_eq!(
            invalid_style.flex_direction(),
            FlexDirectionValue::RowReverse
        );
        assert_eq!(invalid_style.flex_wrap(), FlexWrapValue::Wrap);

        let child_style = document.computed_style_for_layout(child);
        assert_eq!(child_style.flex_direction(), FlexDirectionValue::Row);
        assert_eq!(child_style.flex_wrap(), FlexWrapValue::NoWrap);
    }

    #[test]
    fn justify_content_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { justify-content: space-between; } #explicit { justify-content: flex-end; } #normal { justify-content: normal; } #stretch { justify-content: stretch; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='justify-content: center'>Explicit</span><span id='normal' style='justify-content: safe center'>Normal</span><span id='stretch' style='justify-content: safe center'>Stretch</span><span id='invalid' style='justify-content: safe center'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let normal = document.resolve_target("id=normal").unwrap();
        let stretch = document.resolve_target("id=stretch").unwrap();
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
            document.computed_style_for_layout(normal).justify_content(),
            JustifyContentValue::Normal
        );
        assert_eq!(
            document
                .computed_style_for_layout(stretch)
                .justify_content(),
            JustifyContentValue::Stretch
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .justify_content(),
            JustifyContentValue::FlexStart
        );
    }

    #[test]
    fn align_items_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { align-items: center; } #explicit { align-items: flex-end; } #normal { align-items: normal; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='align-items: flex-start'>Explicit</span><span id='invalid' style='align-items: baseline'>Invalid</span><span id='stretch' style='align-items: stretch'>Stretch</span><span id='normal'>Normal</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let stretch = document.resolve_target("id=stretch").unwrap();
        let normal = document.resolve_target("id=normal").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).align_items(),
            AlignItemsValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(child).align_items(),
            AlignItemsValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).align_items(),
            AlignItemsValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).align_items(),
            AlignItemsValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(stretch).align_items(),
            AlignItemsValue::Stretch
        );
        assert_eq!(
            document.computed_style_for_layout(normal).align_items(),
            AlignItemsValue::Normal
        );
    }

    #[test]
    fn align_self_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { align-items: center; } .item { align-self: flex-end; } #specific { align-self: flex-start; } #later { align-self: center; align-self: flex-end; } #invalid { align-self: center; align-self: baseline; } #stretch { align-self: stretch; } #normal { align-self: normal; }</style><div id='parent'><span id='child'>Child</span><span id='item' class='item'>Item</span><span id='specific' class='item'>Specific</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='stretch'>Stretch</span><span id='normal'>Normal</span><span id='auto' style='align-self: auto'>Auto</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let item = document.resolve_target("id=item").unwrap();
        let specific = document.resolve_target("id=specific").unwrap();
        let later = document.resolve_target("id=later").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let stretch = document.resolve_target("id=stretch").unwrap();
        let normal = document.resolve_target("id=normal").unwrap();
        let auto = document.resolve_target("id=auto").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).align_self(),
            AlignSelfValue::Auto
        );
        assert_eq!(
            document.computed_style_for_layout(child).align_self(),
            AlignSelfValue::Auto
        );
        assert_eq!(
            document.computed_style_for_layout(item).align_self(),
            AlignSelfValue::FlexEnd
        );
        assert_eq!(
            document.computed_style_for_layout(specific).align_self(),
            AlignSelfValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(later).align_self(),
            AlignSelfValue::FlexEnd
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).align_self(),
            AlignSelfValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(stretch).align_self(),
            AlignSelfValue::Stretch
        );
        assert_eq!(
            document.computed_style_for_layout(normal).align_self(),
            AlignSelfValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(auto).align_self(),
            AlignSelfValue::Auto
        );
    }

    #[test]
    fn place_content_is_expanded_with_component_precedence_and_no_inheritance() {
        let document = NativeDocument::parse(
            "<style>#parent { place-content: space-around flex-end; } .shared { place-content: center; } #shared { place-content: flex-start; } #longhands { place-content: stretch center; justify-content: flex-start; align-content: flex-end; } #invalid { place-content: center center; place-content: stretch stretch stretch; } #normal { place-content: normal; } #stretch { place-content: stretch; } #order { align-content: flex-end; place-content: space-between center; justify-content: flex-end; }</style><div id='parent'><span id='child'>Child</span><span id='shared' class='shared'>Shared</span><span id='longhands'>Longhands</span><span id='invalid'>Invalid</span><span id='normal'>Normal</span><span id='stretch'>Stretch</span><span id='order'>Order</span><span id='inline' style='place-content: flex-start flex-end; place-content: center; justify-content: flex-start'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let shared = document.resolve_target("id=shared").unwrap();
        let longhands = document.resolve_target("id=longhands").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let normal = document.resolve_target("id=normal").unwrap();
        let stretch = document.resolve_target("id=stretch").unwrap();
        let order = document.resolve_target("id=order").unwrap();
        let inline = document.resolve_target("id=inline").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).align_content(),
            AlignContentValue::SpaceAround
        );
        assert_eq!(
            document.computed_style_for_layout(parent).justify_content(),
            JustifyContentValue::FlexEnd
        );
        assert_eq!(
            document.computed_style_for_layout(child).align_content(),
            AlignContentValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(child).justify_content(),
            JustifyContentValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(shared).align_content(),
            AlignContentValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(shared).justify_content(),
            JustifyContentValue::FlexStart
        );
        assert_eq!(
            document
                .computed_style_for_layout(longhands)
                .align_content(),
            AlignContentValue::FlexEnd
        );
        assert_eq!(
            document
                .computed_style_for_layout(longhands)
                .justify_content(),
            JustifyContentValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).align_content(),
            AlignContentValue::Center
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .justify_content(),
            JustifyContentValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(normal).align_content(),
            AlignContentValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(normal).justify_content(),
            JustifyContentValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(stretch).align_content(),
            AlignContentValue::Stretch
        );
        assert_eq!(
            document
                .computed_style_for_layout(stretch)
                .justify_content(),
            JustifyContentValue::Stretch
        );
        assert_eq!(
            document.computed_style_for_layout(order).align_content(),
            AlignContentValue::SpaceBetween
        );
        assert_eq!(
            document.computed_style_for_layout(order).justify_content(),
            JustifyContentValue::FlexEnd
        );
        assert_eq!(
            document.computed_style_for_layout(inline).align_content(),
            AlignContentValue::Center
        );
        assert_eq!(
            document.computed_style_for_layout(inline).justify_content(),
            JustifyContentValue::FlexStart
        );
    }

    #[test]
    fn flex_direction_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { flex-direction: row-reverse; } #column { flex-direction: column; } #reverse { flex-direction: column-reverse; } #explicit { flex-direction: row; }</style><div id='parent'><span id='child'>Child</span><span id='column'>Column</span><span id='reverse'>Reverse</span><span id='explicit' style='flex-direction: row'>Explicit</span><span id='invalid' style='flex-direction: row reverse'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let column = document.resolve_target("id=column").unwrap();
        let reverse = document.resolve_target("id=reverse").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).flex_direction(),
            FlexDirectionValue::RowReverse
        );
        assert_eq!(
            document.computed_style_for_layout(child).flex_direction(),
            FlexDirectionValue::Row
        );
        assert_eq!(
            document.computed_style_for_layout(column).flex_direction(),
            FlexDirectionValue::Column
        );
        assert_eq!(
            document.computed_style_for_layout(reverse).flex_direction(),
            FlexDirectionValue::ColumnReverse
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .flex_direction(),
            FlexDirectionValue::Row
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).flex_direction(),
            FlexDirectionValue::Row
        );
    }

    #[test]
    fn direction_is_inherited_and_inline_cascade_can_override_it() {
        let document = NativeDocument::parse(
            "<style>#parent { direction: rtl; } #explicit { direction: rtl; } #invalid { direction: vertical-rl; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='direction: ltr'>Explicit</span><span id='invalid'>Invalid</span><span id='inline-invalid' style='direction: inherit'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();
        let inline_invalid = document.resolve_target("id=inline-invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).direction(),
            DirectionValue::Rtl
        );
        assert_eq!(
            document.computed_style_for_layout(child).direction(),
            DirectionValue::Rtl
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).direction(),
            DirectionValue::Ltr
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).direction(),
            DirectionValue::Rtl
        );
        assert_eq!(
            document
                .computed_style_for_layout(inline_invalid)
                .direction(),
            DirectionValue::Rtl
        );
    }

    #[test]
    fn flex_wrap_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { flex-wrap: wrap-reverse; } #explicit { flex-wrap: nowrap; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='flex-wrap: wrap'>Explicit</span><span id='invalid' style='flex-wrap: wrap reverse'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).flex_wrap(),
            FlexWrapValue::WrapReverse
        );
        assert_eq!(
            document.computed_style_for_layout(child).flex_wrap(),
            FlexWrapValue::NoWrap
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).flex_wrap(),
            FlexWrapValue::Wrap
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).flex_wrap(),
            FlexWrapValue::NoWrap
        );
    }

    #[test]
    fn align_content_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { align-content: normal; } #explicit { align-content: flex-end; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='align-content: space-evenly'>Explicit</span><span id='invalid' style='align-content: safe center'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).align_content(),
            AlignContentValue::Normal
        );
        assert_eq!(
            document.computed_style_for_layout(child).align_content(),
            AlignContentValue::FlexStart
        );
        assert_eq!(
            document.computed_style_for_layout(explicit).align_content(),
            AlignContentValue::SpaceEvenly
        );
        assert_eq!(
            document.computed_style_for_layout(invalid).align_content(),
            AlignContentValue::FlexStart
        );
    }

    #[test]
    fn flex_item_order_is_cascaded_without_inheriting_to_children() {
        let document = NativeDocument::parse(
            "<style>#parent { order: -12; } #explicit { order: 8; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid' style='order: 1025'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let child = document.resolve_target("id=child").unwrap();
        let explicit = document.resolve_target("id=explicit").unwrap();
        let invalid = document.resolve_target("id=invalid").unwrap();

        assert_eq!(
            document.computed_style_for_layout(parent).flex_item_order(),
            NativeOrderValue(-12)
        );
        assert_eq!(
            document.computed_style_for_layout(child).flex_item_order(),
            NativeOrderValue(0)
        );
        assert_eq!(
            document
                .computed_style_for_layout(explicit)
                .flex_item_order(),
            NativeOrderValue(8)
        );
        assert_eq!(
            document
                .computed_style_for_layout(invalid)
                .flex_item_order(),
            NativeOrderValue(0)
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
