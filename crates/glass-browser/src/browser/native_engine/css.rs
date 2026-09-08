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
const MAX_NATIVE_LOCAL_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const IMPORTANT_LOCAL_CASCADE_OFFSET: usize = MAX_NATIVE_CASCADE_LAYERS;
const MAX_NATIVE_TEXT_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const IMPORTANT_TEXT_CASCADE_OFFSET: usize = MAX_NATIVE_CASCADE_LAYERS;
const MAX_NATIVE_RADIUS_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const IMPORTANT_RADIUS_CASCADE_OFFSET: usize = MAX_NATIVE_CASCADE_LAYERS;
const MAX_NATIVE_PAINT_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const IMPORTANT_PAINT_CASCADE_OFFSET: usize = MAX_NATIVE_CASCADE_LAYERS;
const MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS: usize = MAX_NATIVE_CASCADE_LAYERS * 2;
const CASCADE_DECLARATION_ORDER_STRIDE: usize = 256 * 1024 + 1;
const LOGICAL_BORDER_BLOCK_START: usize = 0;
const LOGICAL_BORDER_BLOCK_END: usize = 1;
const LOGICAL_BORDER_INLINE_START: usize = 2;
const LOGICAL_BORDER_INLINE_END: usize = 3;
const LOGICAL_BORDER_SIDES: usize = 4;
const LOGICAL_BORDER_RADIUS_START_START: usize = 0;
const LOGICAL_BORDER_RADIUS_START_END: usize = 1;
const LOGICAL_BORDER_RADIUS_END_START: usize = 2;
const LOGICAL_BORDER_RADIUS_END_END: usize = 3;

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
enum NativeColorValue {
    Color(NativeColor),
    CurrentColor,
    Inherit,
    Unset,
    Initial,
    Revert,
}

impl NativeColorValue {
    const fn resolve(self, inherited: Option<NativeColor>) -> NativeColor {
        match self {
            Self::Color(color) => color,
            Self::CurrentColor | Self::Inherit | Self::Unset | Self::Revert => match inherited {
                Some(color) => color,
                None => NativeColor::BLACK,
            },
            Self::Initial => NativeColor::BLACK,
        }
    }
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum NativeBorderStyleValue {
    #[default]
    None,
    Paint(NativeBorderStyle),
    Hidden,
    Inherit,
    Unset,
    Initial,
    Revert,
}

impl NativeBorderStyleValue {
    const fn is_css_wide(self) -> bool {
        matches!(
            self,
            Self::Inherit | Self::Unset | Self::Initial | Self::Revert
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBorderColorValue {
    Color(NativeColor),
    CurrentColor,
    Inherit,
    Unset,
    Initial,
    Revert,
}

impl NativeBorderColorValue {
    const fn resolve(
        self,
        current_color: NativeColor,
        inherited_color: NativeColor,
    ) -> NativeColor {
        match self {
            Self::Color(color) => color,
            Self::CurrentColor => current_color,
            Self::Inherit => inherited_color,
            Self::Unset | Self::Initial | Self::Revert => current_color,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBorderWidthValue {
    Width(u32),
    Inherit,
    Unset,
    Initial,
    Revert,
}

impl NativeBorderWidthValue {
    const fn is_css_wide(self) -> bool {
        matches!(
            self,
            Self::Inherit | Self::Unset | Self::Initial | Self::Revert
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBackgroundColorValue {
    Color(NativeColor),
    CurrentColor,
    Inherit,
    Unset,
    Initial,
    Revert,
}

impl NativeBackgroundColorValue {
    const fn resolve(
        self,
        inherited_background: Option<NativeColor>,
        current_color: NativeColor,
    ) -> Option<NativeColor> {
        match self {
            Self::Color(color) => Some(color),
            Self::CurrentColor => Some(current_color),
            Self::Inherit => inherited_background,
            Self::Unset | Self::Initial | Self::Revert => None,
        }
    }
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
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl NativeTextDecorationStyleDeclaration {
    const fn resolve(self, inherited: NativeTextDecorationStyle) -> NativeTextDecorationStyle {
        match self {
            Self::Value(value) => value,
            Self::Inherit | Self::Unset | Self::Revert => inherited,
            Self::Initial => NativeTextDecorationStyle::Solid,
            Self::RevertLayer => inherited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationThicknessDeclaration {
    Value(u32),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl NativeTextDecorationThicknessDeclaration {
    const fn resolve(self, inherited: u32) -> u32 {
        match self {
            Self::Value(value) => value,
            Self::Inherit | Self::Unset | Self::Revert => inherited,
            Self::Initial => 1,
            Self::RevertLayer => inherited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextUnderlineOffsetDeclaration {
    Value(i32),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl NativeTextUnderlineOffsetDeclaration {
    const fn resolve(self, inherited: i32) -> i32 {
        match self {
            Self::Value(value) => value,
            Self::Inherit | Self::Unset | Self::Revert => inherited,
            Self::Initial => 0,
            Self::RevertLayer => inherited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationColorDeclaration {
    Value(NativeColor),
    CurrentColor,
    Inherit,
    Unset,
    Initial,
    Revert,
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
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl NativeTextDecorationSkipInkDeclaration {
    const fn resolve(self, inherited: NativeTextDecorationSkipInk) -> NativeTextDecorationSkipInk {
        match self {
            Self::Value(value) => value,
            Self::Inherit | Self::Unset | Self::Revert => inherited,
            Self::Initial => NativeTextDecorationSkipInk::Auto,
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

impl NativeBorderRadius {
    const fn corner(self, index: usize) -> u32 {
        match index {
            0 => self.top_left,
            1 => self.top_right,
            2 => self.bottom_right,
            _ => self.bottom_left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeBorderRadiusValue {
    Radius(NativeBorderRadius),
    Corner(u32),
    Inherit,
    Unset,
    Initial,
    Revert,
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
    CompleteCurrentColor {
        width: u32,
        style: NativeBorderStyle,
    },
    CompleteNone {
        width: u32,
        color: NativeColor,
    },
    CompleteNoneCurrentColor {
        width: u32,
    },
    CompleteHidden {
        width: u32,
        color: NativeColor,
    },
    CompleteHiddenCurrentColor {
        width: u32,
    },
    None,
    Hidden,
    Inherit,
    Unset,
    Initial,
    Revert,
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
    Start,
    End,
    Justify,
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
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl NativeTextDecorationDeclaration {
    const fn resolve(self, inherited: TextDecorationValue) -> TextDecorationValue {
        match self {
            Self::Value(value) => value,
            Self::Inherit | Self::Unset | Self::Revert => inherited,
            Self::Initial => TextDecorationValue::none(),
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
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalCascadeDeclaration<T> {
    Value(T),
    Inherit,
    Reset,
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
pub(crate) enum NativeMarginValue {
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
    pub(crate) background_color: Option<NativeColor>,
    pub(crate) text_decoration_color: NativeColor,
    pub(crate) border_color: [NativeColor; 4],
    pub(crate) border_width: [u32; 4],
    pub(crate) border_style: [NativeBorderStyleValue; 4],
    pub(crate) border_radius: NativeBorderRadius,
    pub(crate) padding: [u32; 4],
    pub(crate) margin: [NativeMarginValue; 4],
    pub(crate) box_sizing: NativeBoxSizing,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) min_width: Option<u32>,
    pub(crate) max_width: Option<u32>,
    pub(crate) min_height: Option<u32>,
    pub(crate) max_height: Option<u32>,
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
            background_color: None,
            text_decoration_color: NativeColor::BLACK,
            border_color: [NativeColor::BLACK; 4],
            border_width: [0; 4],
            border_style: [NativeBorderStyleValue::None; 4],
            border_radius: NativeBorderRadius::default(),
            padding: [0; 4],
            margin: [NativeMarginValue::Length(0); 4],
            box_sizing: NativeBoxSizing::ContentBox,
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
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
    border_colors: Option<[NativeColor; 4]>,
    border_widths: [u32; 4],
    border_styles: [NativeBorderStyleValue; 4],
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
                text_decoration_color: inherited_color.unwrap_or(NativeColor::BLACK),
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
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut visibility: [Option<CascadeValue<LocalCascadeDeclaration<VisibilityValue>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut opacity: [Option<CascadeValue<LocalCascadeDeclaration<u8>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut white_space: [Option<CascadeValue<InheritedTextDeclaration<WhiteSpaceValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_align: [Option<CascadeValue<InheritedTextDeclaration<TextAlignValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_align_last: [Option<
            CascadeValue<InheritedTextDeclaration<TextAlignLastValue>>,
        >; MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_justify: [Option<CascadeValue<InheritedTextDeclaration<TextJustifyValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut direction: [Option<CascadeValue<InheritedTextDeclaration<DirectionValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut justify_content: [Option<CascadeValue<JustifyContentDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut align_items: [Option<CascadeValue<AlignItemsDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut align_self: [Option<CascadeValue<AlignSelfDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut align_content: [Option<CascadeValue<AlignContentDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut flex_direction: [Option<CascadeValue<FlexDirectionDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut flex_wrap: [Option<CascadeValue<FlexWrapDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut flex_item_order: [Option<CascadeValue<FlexItemOrderDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut flex_grow: [Option<CascadeValue<FlexGrowDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut flex_shrink: [Option<CascadeValue<FlexShrinkDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut flex_basis: [Option<CascadeValue<FlexBasisDeclaration>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut text_decoration: [Option<CascadeValue<NativeTextDecorationDeclaration>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_decoration_style: [Option<CascadeValue<NativeTextDecorationStyleDeclaration>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_decoration_skip_ink: [Option<
            CascadeValue<NativeTextDecorationSkipInkDeclaration>,
        >; MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_decoration_skip_spaces: [Option<
            CascadeValue<NativeTextDecorationSkipSpacesDeclaration>,
        >; MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_decoration_thickness: [Option<
            CascadeValue<NativeTextDecorationThicknessDeclaration>,
        >; MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_underline_offset: [Option<CascadeValue<NativeTextUnderlineOffsetDeclaration>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_decoration_color: [Option<CascadeValue<NativeTextDecorationColorDeclaration>>;
            MAX_NATIVE_PAINT_CASCADE_LAYERS] = [None; MAX_NATIVE_PAINT_CASCADE_LAYERS];
        let mut text_transform: [Option<CascadeValue<InheritedTextDeclaration<TextTransformValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut font_weight: [Option<CascadeValue<InheritedTextDeclaration<FontWeightValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut font_style: [Option<CascadeValue<InheritedTextDeclaration<FontStyleValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut word_break: [Option<CascadeValue<InheritedTextDeclaration<WordBreakValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_overflow: [Option<CascadeValue<LocalCascadeDeclaration<TextOverflowValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut vertical_align: [Option<CascadeValue<InheritedTextDeclaration<VerticalAlignValue>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut text_indent: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut word_spacing: [Option<CascadeValue<InheritedTextDeclaration<u32>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut letter_spacing: [Option<CascadeValue<InheritedTextDeclaration<u32>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut gap = GapCascade::default();
        let mut width: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut height: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut min_width: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut max_width: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut min_height: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut max_height: [Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut line_height: [Option<CascadeValue<InheritedTextDeclaration<u32>>>;
            MAX_NATIVE_TEXT_CASCADE_LAYERS] = [None; MAX_NATIVE_TEXT_CASCADE_LAYERS];
        let mut background_color: [Option<
            CascadeValue<LocalCascadeDeclaration<NativeBackgroundColorValue>>,
        >; MAX_NATIVE_PAINT_CASCADE_LAYERS] = [None; MAX_NATIVE_PAINT_CASCADE_LAYERS];
        let mut border: [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderDeclaration>>>;
            MAX_NATIVE_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_CASCADE_LAYERS]; 4];
        let mut logical_border: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderDeclaration>>,
        >; MAX_NATIVE_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES] =
            [[None; MAX_NATIVE_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut border_width: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>,
        >; MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; 4] =
            [[None; MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; 4];
        let mut logical_border_width: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>,
        >; MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS];
            LOGICAL_BORDER_SIDES] =
            [[None; MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut border_style: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>,
        >; MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS]; 4] =
            [[None; MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS]; 4];
        let mut logical_border_style: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>,
        >; MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS];
            LOGICAL_BORDER_SIDES] =
            [[None; MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut border_color: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>,
        >; MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; 4] =
            [[None; MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; 4];
        let mut logical_border_color: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>,
        >; MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS];
            LOGICAL_BORDER_SIDES] =
            [[None; MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut border_radius: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
        >; MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4];
        let mut logical_border_radius: [[Option<
            CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
        >; MAX_NATIVE_RADIUS_CASCADE_LAYERS];
            LOGICAL_BORDER_SIDES] =
            [[None; MAX_NATIVE_RADIUS_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut logical_padding: NativeLogicalBoxModelCandidates<u32> =
            [[None; MAX_NATIVE_LOCAL_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut logical_margin: NativeLogicalBoxModelCandidates<NativeMarginValue> =
            [[None; MAX_NATIVE_LOCAL_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
        let mut padding: [[Option<CascadeValue<LocalCascadeDeclaration<u32>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_LOCAL_CASCADE_LAYERS]; 4];
        let mut margin: [[Option<CascadeValue<LocalCascadeDeclaration<NativeMarginValue>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS]; 4] = [[None; MAX_NATIVE_LOCAL_CASCADE_LAYERS]; 4];
        let mut box_sizing: [Option<CascadeValue<LocalCascadeDeclaration<NativeBoxSizing>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut color: [Option<CascadeValue<LocalCascadeDeclaration<NativeColorValue>>>;
            MAX_NATIVE_PAINT_CASCADE_LAYERS] = [None; MAX_NATIVE_PAINT_CASCADE_LAYERS];
        let mut overflow_x: [Option<CascadeValue<LocalCascadeDeclaration<OverflowValue>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        let mut overflow_y: [Option<CascadeValue<LocalCascadeDeclaration<OverflowValue>>>;
            MAX_NATIVE_LOCAL_CASCADE_LAYERS] = [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
        for rule in &self.rules {
            if !matches(&rule.selector) {
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

        if let Some(inline_style) = node.attribute("style") {
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

        let resolved_direction = resolve_direction(direction, inherited.direction);
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
        project_logical_box_model_candidates(
            &logical_padding,
            resolved_direction,
            inherited.direction,
            inherited.padding,
            &mut padding,
        );
        project_logical_box_model_candidates(
            &logical_margin,
            resolved_direction,
            inherited.direction,
            inherited.margin,
            &mut margin,
        );

        let resolved_padding = std::array::from_fn(|index| {
            resolve_local_inherited_optional_cascade_declaration(
                padding[index],
                inherited.padding[index],
            )
        });
        let resolved_margin = std::array::from_fn(|index| {
            resolve_local_inherited_optional_cascade_declaration(
                margin[index],
                inherited.margin[index],
            )
        });
        let resolved_border = border.map(resolve_local_optional_cascade_declaration);
        let resolved_border_width: [Option<u32>; 4] = std::array::from_fn(|index| {
            resolve_local_border_width_declaration(
                border_width[index],
                inherited.border_width[index],
            )
        });
        let border_widths = std::array::from_fn(|index| resolved_border_width[index].unwrap_or(0));
        let resolved_border_style: [Option<NativeBorderStyleValue>; 4] =
            std::array::from_fn(|index| {
                resolve_local_border_style_declaration(
                    border_style[index],
                    inherited.border_style[index],
                )
            });
        let border_styles = std::array::from_fn(|index| {
            resolved_border_style[index].unwrap_or(NativeBorderStyleValue::None)
        });
        let resolved_color = resolve_local_color_declaration(color, inherited.color);
        let current_color = resolved_color.unwrap_or(NativeColor::BLACK);
        let resolved_background_color = resolve_local_background_color_declaration(
            background_color,
            inherited.background_color,
            current_color,
        );
        let resolved_border_color: [Option<NativeColor>; 4] = std::array::from_fn(|index| {
            resolve_local_border_color_declaration(
                border_color[index],
                current_color,
                inherited.border_color[index],
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
            resolve_local_inherited_cascade_declaration(box_sizing, inherited.box_sizing);
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
            direction: resolved_direction,
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
            text_decoration_color: resolve_text_decoration_color(
                text_decoration_color,
                current_color,
                inherited.text_decoration_color,
            ),
            text_transform: resolve_inherited_text_declaration(
                text_transform,
                inherited.text_transform,
                TextTransformValue::None,
            ),
            font_weight: resolve_inherited_text_declaration(
                font_weight,
                inherited.font_weight,
                FontWeightValue::Normal,
            ),
            font_style: resolve_inherited_text_declaration(
                font_style,
                inherited.font_style,
                FontStyleValue::Normal,
            ),
            word_break: resolve_inherited_text_declaration(
                word_break,
                inherited.word_break,
                WordBreakValue::Normal,
            ),
            text_overflow: resolve_local_cascade_declaration(
                text_overflow,
                TextOverflowValue::Clip,
            ),
            vertical_align: resolve_inherited_text_declaration(
                vertical_align,
                inherited.vertical_align,
                VerticalAlignValue::Baseline,
            ),
            text_indent: resolve_local_cascade_declaration(text_indent, 0),
            word_spacing: resolve_inherited_text_declaration(
                word_spacing,
                inherited.word_spacing,
                0,
            ),
            letter_spacing: resolve_inherited_text_declaration(
                letter_spacing,
                inherited.letter_spacing,
                0,
            ),
            gap: resolve_gap_axis(gap.shorthand_column, gap.column_gap),
            row_gap: resolve_gap_axis(gap.shorthand_row, gap.row_gap),
            width: resolve_local_inherited_nullable_cascade_declaration(width, inherited.width),
            height: resolve_local_inherited_nullable_cascade_declaration(height, inherited.height),
            min_width: resolve_local_inherited_nullable_cascade_declaration(
                min_width,
                inherited.min_width,
            ),
            max_width: resolve_local_inherited_nullable_cascade_declaration(
                max_width,
                inherited.max_width,
            ),
            min_height: resolve_local_inherited_nullable_cascade_declaration(
                min_height,
                inherited.min_height,
            ),
            max_height: resolve_local_inherited_nullable_cascade_declaration(
                max_height,
                inherited.max_height,
            ),
            line_height: resolve_line_height(line_height, inherited.line_height),
            background_color: resolved_background_color,
            border: NativeBorder::from_sides(resolved_border),
            border_colors: Some(border_colors),
            border_widths,
            border_styles,
            border_radius: resolve_local_border_radius_declaration(
                border_radius,
                inherited.border_radius,
            ),
            padding: NativeBoxEdges::from_values(resolved_padding),
            margin: NativeBoxEdges::from_margin_values(resolved_margin),
            margin_auto: NativeAutoEdges::from_values(resolved_margin),
            box_sizing: resolved_box_sizing,
            color: resolved_color,
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
    shorthand_row:
        [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    shorthand_column:
        [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    row_gap: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    column_gap: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
}

impl Default for GapCascade {
    fn default() -> Self {
        Self {
            shorthand_row: [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
            shorthand_column: [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
            row_gap: [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
            column_gap: [None; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
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
    candidates: [Option<CascadeValue<InheritedTextDeclaration<WhiteSpaceValue>>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: WhiteSpaceValue,
) -> WhiteSpaceValue {
    resolve_inherited_text_declaration(candidates, inherited, WhiteSpaceValue::Normal)
}

fn resolve_line_height(
    candidates: [Option<CascadeValue<InheritedTextDeclaration<u32>>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: Option<u32>,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        InheritedTextDeclaration::Value(value) => Some(Some(value)),
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(inherited),
        InheritedTextDeclaration::Initial => Some(None),
        InheritedTextDeclaration::RevertLayer => None,
    })
}

fn resolve_direction(
    candidates: [Option<CascadeValue<InheritedTextDeclaration<DirectionValue>>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: DirectionValue,
) -> DirectionValue {
    resolve_inherited_text_declaration(candidates, inherited, DirectionValue::Ltr)
}

fn resolve_flex_direction(
    candidates: [Option<CascadeValue<FlexDirectionDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> FlexDirectionValue {
    resolve_alignment_candidates(candidates, FlexDirectionValue::Row, |declaration| {
        match declaration {
            FlexDirectionDeclaration::Value(value) => Some(value),
            FlexDirectionDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_justify_content(
    candidates: [Option<CascadeValue<JustifyContentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> JustifyContentValue {
    resolve_alignment_candidates(candidates, JustifyContentValue::FlexStart, |declaration| {
        match declaration {
            JustifyContentDeclaration::Value(value) => Some(value),
            JustifyContentDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_align_items(
    candidates: [Option<CascadeValue<AlignItemsDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> AlignItemsValue {
    resolve_alignment_candidates(candidates, AlignItemsValue::FlexStart, |declaration| {
        match declaration {
            AlignItemsDeclaration::Value(value) => Some(value),
            AlignItemsDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_align_self(
    candidates: [Option<CascadeValue<AlignSelfDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
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
    candidates: [Option<CascadeValue<AlignContentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> AlignContentValue {
    resolve_alignment_candidates(candidates, AlignContentValue::FlexStart, |declaration| {
        match declaration {
            AlignContentDeclaration::Value(value) => Some(value),
            AlignContentDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_flex_wrap(
    candidates: [Option<CascadeValue<FlexWrapDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
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
    candidates: [Option<CascadeValue<FlexItemOrderDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> NativeOrderValue {
    resolve_alignment_candidates(candidates, NativeOrderValue::default(), |declaration| {
        match declaration {
            FlexItemOrderDeclaration::Value(value) => Some(value),
            FlexItemOrderDeclaration::RevertLayer => None,
        }
    })
}

fn resolve_flex_grow(
    candidates: [Option<CascadeValue<FlexGrowDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> u32 {
    resolve_alignment_candidates(candidates, 0, |declaration| match declaration {
        FlexGrowDeclaration::Value(value) => Some(value),
        FlexGrowDeclaration::RevertLayer => None,
    })
}

fn resolve_flex_shrink(
    candidates: [Option<CascadeValue<FlexShrinkDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> u32 {
    resolve_alignment_candidates(candidates, 1, |declaration| match declaration {
        FlexShrinkDeclaration::Value(value) => Some(value),
        FlexShrinkDeclaration::RevertLayer => None,
    })
}

fn resolve_flex_basis(
    candidates: [Option<CascadeValue<FlexBasisDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
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
    candidates: [Option<CascadeValue<InheritedTextDeclaration<TextAlignValue>>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: TextAlignValue,
) -> TextAlignValue {
    resolve_inherited_text_declaration(candidates, inherited, TextAlignValue::Left)
}

fn resolve_text_align_last(
    candidates: [Option<CascadeValue<InheritedTextDeclaration<TextAlignLastValue>>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: TextAlignLastValue,
) -> TextAlignLastValue {
    resolve_inherited_text_declaration(candidates, inherited, TextAlignLastValue::Auto)
}

fn resolve_text_justify(
    candidates: [Option<CascadeValue<InheritedTextDeclaration<TextJustifyValue>>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: TextJustifyValue,
) -> TextJustifyValue {
    resolve_inherited_text_declaration(candidates, inherited, TextJustifyValue::Auto)
}

fn resolve_inherited_text_declaration<T: Copy>(
    candidates: [Option<CascadeValue<InheritedTextDeclaration<T>>>; MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: T,
    initial: T,
) -> T {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        InheritedTextDeclaration::Value(value) => Some(value),
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(inherited),
        InheritedTextDeclaration::Initial => Some(initial),
        InheritedTextDeclaration::RevertLayer => None,
    })
}

fn resolve_local_cascade_declaration<T: Copy, const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; N],
    fallback: T,
) -> T {
    resolve_alignment_candidates(candidates, fallback, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(value),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => None,
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_optional_cascade_declaration<T: Copy, const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; N],
) -> Option<T> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(value)),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_color_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeColorValue>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    inherited: Option<NativeColor>,
) -> Option<NativeColor> {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(value.resolve(inherited))),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_background_color_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBackgroundColorValue>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    inherited_background: Option<NativeColor>,
    current_color: NativeColor,
) -> Option<NativeColor> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => {
            Some(value.resolve(inherited_background, current_color))
        }
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_border_color_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>>;
        MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS],
    current_color: NativeColor,
    inherited_color: NativeColor,
) -> Option<NativeColor> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => {
            Some(Some(value.resolve(current_color, inherited_color)))
        }
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_border_width_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>>;
        MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS],
    inherited_width: u32,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(match value {
            NativeBorderWidthValue::Width(width) => width,
            NativeBorderWidthValue::Inherit => inherited_width,
            NativeBorderWidthValue::Unset
            | NativeBorderWidthValue::Initial
            | NativeBorderWidthValue::Revert => 0,
        })),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(Some(0)),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_border_style_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>>;
        MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS],
    inherited_style: NativeBorderStyleValue,
) -> Option<NativeBorderStyleValue> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(match value {
            NativeBorderStyleValue::Paint(_)
            | NativeBorderStyleValue::None
            | NativeBorderStyleValue::Hidden => value,
            NativeBorderStyleValue::Inherit => inherited_style,
            NativeBorderStyleValue::Unset
            | NativeBorderStyleValue::Initial
            | NativeBorderStyleValue::Revert => NativeBorderStyleValue::None,
        })),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(Some(NativeBorderStyleValue::None)),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_border_radius_declaration(
    candidates: [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
        MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4],
    inherited_radius: NativeBorderRadius,
) -> NativeBorderRadius {
    let [top_left, top_right, bottom_right, bottom_left] = std::array::from_fn(|index| {
        resolve_alignment_candidates(candidates[index], None, |declaration| match declaration {
            LocalCascadeDeclaration::Value(value) => Some(Some(match value {
                NativeBorderRadiusValue::Radius(radius) => radius.corner(index),
                NativeBorderRadiusValue::Corner(radius) => radius,
                NativeBorderRadiusValue::Inherit => inherited_radius.corner(index),
                NativeBorderRadiusValue::Unset
                | NativeBorderRadiusValue::Initial
                | NativeBorderRadiusValue::Revert => 0,
            })),
            LocalCascadeDeclaration::Inherit => None,
            LocalCascadeDeclaration::Reset => Some(Some(0)),
            LocalCascadeDeclaration::RevertLayer => None,
        })
        .unwrap_or_default()
    });
    NativeBorderRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

fn resolve_local_inherited_optional_cascade_declaration<T: Copy, const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; N],
    inherited: T,
) -> Option<T> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(value)),
        LocalCascadeDeclaration::Inherit => Some(Some(inherited)),
        LocalCascadeDeclaration::Reset => None,
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_inherited_nullable_cascade_declaration<T: Copy, const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; N],
    inherited: Option<T>,
) -> Option<T> {
    resolve_alignment_candidates(candidates, None, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(Some(value)),
        LocalCascadeDeclaration::Inherit => Some(inherited),
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_local_inherited_cascade_declaration<T: Copy, const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<T>>>; N],
    inherited: T,
) -> T {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => Some(value),
        LocalCascadeDeclaration::Inherit => Some(inherited),
        LocalCascadeDeclaration::Reset => None,
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

fn resolve_alignment_candidates<T: Copy, U: Copy, const N: usize>(
    candidates: [Option<CascadeValue<T>>; N],
    inherited: U,
    value: impl Fn(T) -> Option<U>,
) -> U {
    let mut blocked = [false; N];
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
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: NativeTextDecorationSkipSpaces,
) -> NativeTextDecorationSkipSpaces {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
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
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: NativeTextDecorationStyle,
) -> NativeTextDecorationStyle {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
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
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: u32,
) -> u32 {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
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
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: i32,
) -> i32 {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
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
    candidates: [Option<CascadeValue<NativeTextDecorationDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: TextDecorationValue,
) -> TextDecorationValue {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
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
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    current_color: NativeColor,
    inherited_color: NativeColor,
) -> Option<NativeColor> {
    let mut blocked = [false; MAX_NATIVE_PAINT_CASCADE_LAYERS];
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
            NativeTextDecorationColorDeclaration::CurrentColor => Some(current_color),
            NativeTextDecorationColorDeclaration::Inherit => Some(inherited_color),
            NativeTextDecorationColorDeclaration::Unset
            | NativeTextDecorationColorDeclaration::Initial
            | NativeTextDecorationColorDeclaration::Revert => Some(current_color),
            NativeTextDecorationColorDeclaration::RevertLayer => unreachable!(),
        };
    }
}

fn resolve_text_decoration_skip_ink(
    candidates: [Option<CascadeValue<NativeTextDecorationSkipInkDeclaration>>;
        MAX_NATIVE_TEXT_CASCADE_LAYERS],
    inherited: NativeTextDecorationSkipInk,
) -> NativeTextDecorationSkipInk {
    let mut blocked = [false; MAX_NATIVE_TEXT_CASCADE_LAYERS];
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
    shorthand: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
    longhand: [Option<GapCascadeValue<GapComponentDeclaration>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) -> u32 {
    let mut blocked = [false; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
    loop {
        let Some((layer, candidate)) =
            (0..MAX_NATIVE_LOCAL_CASCADE_LAYERS)
                .rev()
                .find_map(|layer| {
                    if blocked[layer] {
                        None
                    } else {
                        select_gap_candidate(shorthand[layer], longhand[layer])
                            .map(|candidate| (layer, candidate))
                    }
                })
        else {
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
    if let Some(value) = declarations.gap {
        let layer = local_cascade_layer(specificity, declarations.gap_important);
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
        let layer = local_cascade_layer(specificity, declarations.row_gap_important);
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
        let layer = local_cascade_layer(specificity, declarations.column_gap_important);
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

fn text_cascade_layer(specificity: u16, important: bool) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_TEXT_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

fn apply_text_cascade_declaration<T: Copy>(
    declaration: Option<T>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<T>>; MAX_NATIVE_TEXT_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = text_cascade_layer(specificity, important);
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

fn local_cascade_layer(specificity: u16, important: bool) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_LOCAL_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

fn apply_local_important_cascade_declaration<T: Copy>(
    declaration: Option<LocalCascadeDeclaration<T>>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<T>>>;
             MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = local_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

fn apply_flex_cascade_declaration<T: Copy>(
    declaration: Option<T>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<T>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = local_cascade_layer(specificity, important);
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

fn radius_cascade_layer(specificity: u16, important: bool) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_RADIUS_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

fn apply_radius_cascade_declaration(
    declaration: Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
             MAX_NATIVE_RADIUS_CASCADE_LAYERS],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = radius_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

fn paint_cascade_layer(specificity: u16, important: bool) -> usize {
    let layer = cascade_layer_index(specificity);
    if important {
        IMPORTANT_PAINT_CASCADE_OFFSET
            .saturating_add(usize::from(UNLAYERED_CASCADE_LAYER).saturating_sub(layer))
    } else {
        layer
    }
}

fn apply_paint_cascade_declaration<T: Copy, const N: usize>(
    declaration: Option<T>,
    specificity: u16,
    order: usize,
    inline: bool,
    important: bool,
    candidates: &mut [Option<CascadeValue<T>>; N],
) {
    let Some(value) = declaration else {
        return;
    };
    let layer = paint_cascade_layer(specificity, important);
    if wins(specificity, order, inline, candidates[layer]) {
        candidates[layer] = Some(CascadeValue {
            value,
            specificity,
            order,
            inline,
        });
    }
}

fn project_border_width_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<NativeBorderWidthValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => Some(
            LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(border.width())),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteCurrentColor { width, .. }
            | NativeBorderDeclaration::CompleteNoneCurrentColor { width }
            | NativeBorderDeclaration::CompleteHiddenCurrentColor { width }
            | NativeBorderDeclaration::CompleteHidden { width, .. }
            | NativeBorderDeclaration::CompleteNone { width, .. },
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderWidthValue::Width(width),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None | NativeBorderDeclaration::Hidden,
        ) => None,
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Inherit) => Some(
            LocalCascadeDeclaration::Value(NativeBorderWidthValue::Inherit),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Unset
            | NativeBorderDeclaration::Initial
            | NativeBorderDeclaration::Revert,
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderWidthValue::Width(0),
        )),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(LocalCascadeDeclaration::Reset),
        LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
    }
}

fn project_border_style_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<NativeBorderStyleValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => Some(
            LocalCascadeDeclaration::Value(NativeBorderStyleValue::Paint(border.style())),
        ),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::CompleteCurrentColor {
            style,
            ..
        }) => Some(LocalCascadeDeclaration::Value(
            NativeBorderStyleValue::Paint(style),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteHidden { .. }
            | NativeBorderDeclaration::CompleteHiddenCurrentColor { .. }
            | NativeBorderDeclaration::Hidden,
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderStyleValue::Hidden,
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteNone { .. }
            | NativeBorderDeclaration::CompleteNoneCurrentColor { .. }
            | NativeBorderDeclaration::None,
        ) => Some(LocalCascadeDeclaration::Value(NativeBorderStyleValue::None)),
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Inherit) => Some(
            LocalCascadeDeclaration::Value(NativeBorderStyleValue::Inherit),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Unset
            | NativeBorderDeclaration::Initial
            | NativeBorderDeclaration::Revert,
        ) => Some(LocalCascadeDeclaration::Value(NativeBorderStyleValue::None)),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(LocalCascadeDeclaration::Reset),
        LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
    }
}

fn project_border_color_declaration(
    declaration: LocalCascadeDeclaration<NativeBorderDeclaration>,
) -> Option<LocalCascadeDeclaration<NativeBorderColorValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Complete(border)) => Some(
            LocalCascadeDeclaration::Value(NativeBorderColorValue::Color(border.color())),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteCurrentColor { .. }
            | NativeBorderDeclaration::CompleteHiddenCurrentColor { .. }
            | NativeBorderDeclaration::CompleteNoneCurrentColor { .. },
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CurrentColor,
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteHidden { color, .. }
            | NativeBorderDeclaration::CompleteNone { color, .. },
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::Color(color),
        )),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None | NativeBorderDeclaration::Hidden,
        ) => None,
        LocalCascadeDeclaration::Value(NativeBorderDeclaration::Inherit) => Some(
            LocalCascadeDeclaration::Value(NativeBorderColorValue::Inherit),
        ),
        LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Unset
            | NativeBorderDeclaration::Initial
            | NativeBorderDeclaration::Revert,
        ) => Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CurrentColor,
        )),
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(LocalCascadeDeclaration::Reset),
        LocalCascadeDeclaration::RevertLayer => Some(LocalCascadeDeclaration::RevertLayer),
    }
}

fn apply_border_width_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativePhysicalBorderWidthCandidates,
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_width = declarations.border[index].and_then(project_border_width_declaration);
        apply_paint_cascade_declaration(
            border_width,
            specificity,
            border_cascade_order(rule_order, declarations.border_order[index], inline),
            inline,
            declarations.border_important[index],
            candidate,
        );
        apply_paint_cascade_declaration(
            declarations.border_width[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_width_order[index], inline),
            inline,
            declarations.border_width_important[index],
            candidate,
        );
    }
}

fn apply_border_style_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativePhysicalBorderStyleCandidates,
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_style = declarations.border[index].and_then(project_border_style_declaration);
        apply_paint_cascade_declaration(
            border_style,
            specificity,
            border_cascade_order(rule_order, declarations.border_order[index], inline),
            inline,
            declarations.border_important[index],
            candidate,
        );
        apply_paint_cascade_declaration(
            declarations.border_style[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_style_order[index], inline),
            inline,
            declarations.border_style_important[index],
            candidate,
        );
    }
}

fn apply_border_color_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>>;
             MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; 4],
) {
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let border_order =
            border_cascade_order(rule_order, declarations.border_order[index], inline);
        let border_color = declarations.border[index].and_then(project_border_color_declaration);
        apply_paint_cascade_declaration(
            border_color,
            specificity,
            border_order,
            inline,
            declarations.border_important[index],
            candidate,
        );
        apply_paint_cascade_declaration(
            declarations.border_color[index],
            specificity,
            border_cascade_order(rule_order, declarations.border_color_order[index], inline),
            inline,
            declarations.border_color_important[index],
            candidate,
        );
    }
}

fn apply_border_radius_cascade(
    declarations: &NativeDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
             MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4],
) {
    if let Some(border_radius) = declarations.border_radius {
        let order = border_cascade_order(rule_order, declarations.border_radius_order, inline);
        for candidate in candidates.iter_mut() {
            apply_radius_cascade_declaration(
                Some(border_radius),
                specificity,
                order,
                inline,
                declarations.border_radius_important,
                candidate,
            );
        }
    }
    for (index, candidate) in candidates.iter_mut().enumerate() {
        apply_radius_cascade_declaration(
            declarations.border_radius_corners[index],
            specificity,
            border_cascade_order(
                rule_order,
                declarations.border_radius_corner_orders[index],
                inline,
            ),
            inline,
            declarations.border_radius_corner_important[index],
            candidate,
        );
    }
}

fn apply_logical_border_radius_cascade(
    declarations: &NativeLogicalBorderRadiusDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativeLogicalBorderRadiusCandidates,
) {
    for (index, declaration) in declarations.corners.iter().copied().enumerate() {
        apply_radius_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declarations.corner_orders[index], inline),
            inline,
            declarations.important[index],
            &mut candidates[index],
        );
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeLogicalBorderDeclarations {
    border: [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; LOGICAL_BORDER_SIDES],
    border_order: [usize; LOGICAL_BORDER_SIDES],
    border_important: [bool; LOGICAL_BORDER_SIDES],
    width: [Option<LocalCascadeDeclaration<NativeBorderWidthValue>>; LOGICAL_BORDER_SIDES],
    width_order: [usize; LOGICAL_BORDER_SIDES],
    width_important: [bool; LOGICAL_BORDER_SIDES],
    style: [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; LOGICAL_BORDER_SIDES],
    style_order: [usize; LOGICAL_BORDER_SIDES],
    style_important: [bool; LOGICAL_BORDER_SIDES],
    color: [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; LOGICAL_BORDER_SIDES],
    color_order: [usize; LOGICAL_BORDER_SIDES],
    color_important: [bool; LOGICAL_BORDER_SIDES],
}

impl NativeLogicalBorderDeclarations {
    fn has_any(self) -> bool {
        self.border.iter().any(Option::is_some)
            || self.width.iter().any(Option::is_some)
            || self.style.iter().any(Option::is_some)
            || self.color.iter().any(Option::is_some)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeLogicalBorderRadiusDeclarations {
    corners: [Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>; LOGICAL_BORDER_SIDES],
    corner_orders: [usize; LOGICAL_BORDER_SIDES],
    important: [bool; LOGICAL_BORDER_SIDES],
}

impl NativeLogicalBorderRadiusDeclarations {
    fn has_any(self) -> bool {
        self.corners.iter().any(Option::is_some)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeLogicalBoxModelDeclarations {
    padding: [Option<LocalCascadeDeclaration<u32>>; LOGICAL_BORDER_SIDES],
    padding_order: [usize; LOGICAL_BORDER_SIDES],
    padding_important: [bool; LOGICAL_BORDER_SIDES],
    margin: [Option<LocalCascadeDeclaration<NativeMarginValue>>; LOGICAL_BORDER_SIDES],
    margin_order: [usize; LOGICAL_BORDER_SIDES],
    margin_important: [bool; LOGICAL_BORDER_SIDES],
}

impl NativeLogicalBoxModelDeclarations {
    fn has_any(self) -> bool {
        self.padding.iter().any(Option::is_some) || self.margin.iter().any(Option::is_some)
    }
}

type NativeLogicalBorderCandidates<T> = [[Option<CascadeValue<LocalCascadeDeclaration<T>>>;
    MAX_NATIVE_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
type NativeLogicalBorderWidthCandidates =
    [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>>;
        MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
type NativeLogicalBorderStyleCandidates =
    [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>>;
        MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
type NativeLogicalBorderColorCandidates =
    [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>>;
        MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
type NativeLogicalBorderRadiusCandidates =
    [[Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
        MAX_NATIVE_RADIUS_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
type NativeLogicalBoxModelCandidates<T> = [[Option<CascadeValue<LocalCascadeDeclaration<T>>>;
    MAX_NATIVE_LOCAL_CASCADE_LAYERS];
    LOGICAL_BORDER_SIDES];
type NativePhysicalBorderCandidates<T> =
    [[Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS]; 4];
type NativePhysicalBorderColorCandidates = [[Option<
    CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>,
>; MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS]; 4];
type NativePhysicalBorderWidthCandidates = [[Option<
    CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>,
>; MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; 4];
type NativePhysicalBorderStyleCandidates = [[Option<
    CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>,
>; MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS]; 4];
type NativePhysicalBorderRadiusCandidates = [[Option<
    CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
>; MAX_NATIVE_RADIUS_CASCADE_LAYERS]; 4];

struct NativeLogicalBorderCandidateSources<'a> {
    border: &'a NativeLogicalBorderCandidates<NativeBorderDeclaration>,
    width: &'a NativeLogicalBorderWidthCandidates,
    style: &'a NativeLogicalBorderStyleCandidates,
    color: &'a NativeLogicalBorderColorCandidates,
}

struct NativeLogicalBorderCandidateTargets<'a> {
    border: &'a mut NativeLogicalBorderCandidates<NativeBorderDeclaration>,
    width: &'a mut NativeLogicalBorderWidthCandidates,
    style: &'a mut NativeLogicalBorderStyleCandidates,
    color: &'a mut NativeLogicalBorderColorCandidates,
}

struct NativePhysicalBorderCandidateTargets<'a> {
    border: &'a mut NativePhysicalBorderCandidates<NativeBorderDeclaration>,
    width: &'a mut NativePhysicalBorderWidthCandidates,
    style: &'a mut NativePhysicalBorderStyleCandidates,
    color: &'a mut NativePhysicalBorderColorCandidates,
}

fn merge_cascade_candidate<T: Copy, const N: usize>(
    candidate: CascadeValue<T>,
    candidates: &mut [Option<CascadeValue<T>>; N],
) {
    let layer = cascade_layer_index(candidate.specificity);
    if wins(
        candidate.specificity,
        candidate.order,
        candidate.inline,
        candidates[layer],
    ) {
        candidates[layer] = Some(candidate);
    }
}

fn merge_border_color_cascade_candidate(
    candidate: CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>,
    layer: usize,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderColorValue>>>;
             MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS],
) {
    if wins(
        candidate.specificity,
        candidate.order,
        candidate.inline,
        candidates[layer],
    ) {
        candidates[layer] = Some(candidate);
    }
}

fn merge_border_width_cascade_candidate(
    candidate: CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>,
    layer: usize,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderWidthValue>>>;
             MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS],
) {
    if wins(
        candidate.specificity,
        candidate.order,
        candidate.inline,
        candidates[layer],
    ) {
        candidates[layer] = Some(candidate);
    }
}

fn merge_border_style_cascade_candidate(
    candidate: CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>,
    layer: usize,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderStyleValue>>>;
             MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS],
) {
    if wins(
        candidate.specificity,
        candidate.order,
        candidate.inline,
        candidates[layer],
    ) {
        candidates[layer] = Some(candidate);
    }
}

fn merge_radius_cascade_candidate(
    candidate: CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
    layer: usize,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<NativeBorderRadiusValue>>>;
             MAX_NATIVE_RADIUS_CASCADE_LAYERS],
) {
    if wins(
        candidate.specificity,
        candidate.order,
        candidate.inline,
        candidates[layer],
    ) {
        candidates[layer] = Some(candidate);
    }
}

fn logical_border_physical_side(logical_side: usize, direction: DirectionValue) -> usize {
    match logical_side {
        LOGICAL_BORDER_BLOCK_START => 0,
        LOGICAL_BORDER_BLOCK_END => 2,
        LOGICAL_BORDER_INLINE_START if direction == DirectionValue::Ltr => 3,
        LOGICAL_BORDER_INLINE_START => 1,
        LOGICAL_BORDER_INLINE_END if direction == DirectionValue::Ltr => 1,
        LOGICAL_BORDER_INLINE_END => 3,
        _ => 0,
    }
}

fn logical_border_radius_physical_corner(
    logical_corner: usize,
    direction: DirectionValue,
) -> usize {
    match logical_corner {
        LOGICAL_BORDER_RADIUS_START_START if direction == DirectionValue::Ltr => 0,
        LOGICAL_BORDER_RADIUS_START_START => 1,
        LOGICAL_BORDER_RADIUS_START_END if direction == DirectionValue::Ltr => 1,
        LOGICAL_BORDER_RADIUS_START_END => 0,
        LOGICAL_BORDER_RADIUS_END_START if direction == DirectionValue::Ltr => 3,
        LOGICAL_BORDER_RADIUS_END_START => 2,
        LOGICAL_BORDER_RADIUS_END_END if direction == DirectionValue::Ltr => 2,
        LOGICAL_BORDER_RADIUS_END_END => 3,
        _ => 0,
    }
}

fn project_logical_border_candidates(
    sources: NativeLogicalBorderCandidateSources<'_>,
    direction: DirectionValue,
    targets: NativePhysicalBorderCandidateTargets<'_>,
) {
    for logical_side in 0..LOGICAL_BORDER_SIDES {
        let physical_side = logical_border_physical_side(logical_side, direction);
        for layer in 0..MAX_NATIVE_CASCADE_LAYERS {
            if let Some(candidate) = sources.border[logical_side][layer] {
                merge_cascade_candidate(candidate, &mut targets.border[physical_side]);
                if let Some(value) = project_border_width_declaration(candidate.value) {
                    merge_cascade_candidate(
                        CascadeValue {
                            value,
                            specificity: candidate.specificity,
                            order: candidate.order,
                            inline: candidate.inline,
                        },
                        &mut targets.width[physical_side],
                    );
                }
                if let Some(value) = project_border_style_declaration(candidate.value) {
                    merge_cascade_candidate(
                        CascadeValue {
                            value,
                            specificity: candidate.specificity,
                            order: candidate.order,
                            inline: candidate.inline,
                        },
                        &mut targets.style[physical_side],
                    );
                }
                if let Some(value) = project_border_color_declaration(candidate.value) {
                    merge_border_color_cascade_candidate(
                        CascadeValue {
                            value,
                            specificity: candidate.specificity,
                            order: candidate.order,
                            inline: candidate.inline,
                        },
                        layer,
                        &mut targets.color[physical_side],
                    );
                }
            }
            if let Some(candidate) = sources.width[logical_side][layer] {
                merge_cascade_candidate(candidate, &mut targets.width[physical_side]);
            }
            if let Some(candidate) = sources.style[logical_side][layer] {
                merge_cascade_candidate(candidate, &mut targets.style[physical_side]);
            }
            if let Some(candidate) = sources.color[logical_side][layer] {
                merge_border_color_cascade_candidate(
                    candidate,
                    layer,
                    &mut targets.color[physical_side],
                );
            }
        }
        for layer in MAX_NATIVE_CASCADE_LAYERS..MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS {
            if let Some(candidate) = sources.width[logical_side][layer] {
                merge_border_width_cascade_candidate(
                    candidate,
                    layer,
                    &mut targets.width[physical_side],
                );
            }
        }
        for layer in MAX_NATIVE_CASCADE_LAYERS..MAX_NATIVE_BORDER_STYLE_CASCADE_LAYERS {
            if let Some(candidate) = sources.style[logical_side][layer] {
                merge_border_style_cascade_candidate(
                    candidate,
                    layer,
                    &mut targets.style[physical_side],
                );
            }
        }
        for layer in MAX_NATIVE_CASCADE_LAYERS..MAX_NATIVE_BORDER_COLOR_CASCADE_LAYERS {
            if let Some(candidate) = sources.color[logical_side][layer] {
                merge_border_color_cascade_candidate(
                    candidate,
                    layer,
                    &mut targets.color[physical_side],
                );
            }
        }
    }
}

fn project_logical_border_radius_candidates(
    sources: &NativeLogicalBorderRadiusCandidates,
    direction: DirectionValue,
    targets: &mut NativePhysicalBorderRadiusCandidates,
) {
    for (logical_corner, logical_candidates) in sources.iter().enumerate() {
        let physical_corner = logical_border_radius_physical_corner(logical_corner, direction);
        for (layer, candidate) in logical_candidates.iter().copied().enumerate() {
            if let Some(candidate) = candidate {
                merge_radius_cascade_candidate(candidate, layer, &mut targets[physical_corner]);
            }
        }
    }
}

fn project_logical_box_model_candidates<T: Copy>(
    sources: &NativeLogicalBoxModelCandidates<T>,
    direction: DirectionValue,
    inherited_direction: DirectionValue,
    inherited: [T; 4],
    targets: &mut [[Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
             4],
) {
    for (logical_side, logical_candidates) in sources.iter().enumerate() {
        let physical_side = logical_border_physical_side(logical_side, direction);
        let inherited_physical_side =
            logical_border_physical_side(logical_side, inherited_direction);
        for (layer, candidate) in logical_candidates.iter().copied().enumerate() {
            let Some(mut candidate) = candidate else {
                continue;
            };
            if matches!(candidate.value, LocalCascadeDeclaration::Inherit) {
                candidate.value =
                    LocalCascadeDeclaration::Value(inherited[inherited_physical_side]);
            }
            if wins(
                candidate.specificity,
                candidate.order,
                candidate.inline,
                targets[physical_side][layer],
            ) {
                targets[physical_side][layer] = Some(candidate);
            }
        }
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

fn apply_local_important_cascade_edges<T: Copy>(
    declarations: &[Option<LocalCascadeDeclaration<T>>; 4],
    importance: &[bool; 4],
    declaration_orders: &[usize; 4],
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
             4],
) {
    for (index, declaration) in declarations.iter().copied().enumerate() {
        apply_local_important_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declaration_orders[index], inline),
            inline,
            importance[index],
            &mut candidates[index],
        );
    }
}

fn apply_logical_box_model_cascade(
    declarations: &NativeLogicalBoxModelDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    padding: &mut NativeLogicalBoxModelCandidates<u32>,
    margin: &mut NativeLogicalBoxModelCandidates<NativeMarginValue>,
) {
    for index in 0..LOGICAL_BORDER_SIDES {
        let order = border_cascade_order(rule_order, declarations.padding_order[index], inline);
        apply_local_important_cascade_declaration(
            declarations.padding[index],
            specificity,
            order,
            inline,
            declarations.padding_important[index],
            &mut padding[index],
        );

        let order = border_cascade_order(rule_order, declarations.margin_order[index], inline);
        apply_local_important_cascade_declaration(
            declarations.margin[index],
            specificity,
            order,
            inline,
            declarations.margin_important[index],
            &mut margin[index],
        );
    }
}

fn apply_logical_cascade_edges<T: Copy>(
    declarations: &[Option<LocalCascadeDeclaration<T>>; LOGICAL_BORDER_SIDES],
    declaration_orders: &[usize; LOGICAL_BORDER_SIDES],
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut [[Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_CASCADE_LAYERS];
             LOGICAL_BORDER_SIDES],
) {
    for (index, declaration) in declarations.iter().copied().enumerate() {
        apply_local_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declaration_orders[index], inline),
            inline,
            &mut candidates[index],
        );
    }
}

fn apply_logical_border_shorthand_cascade(
    declarations: &NativeLogicalBorderDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    width: &mut NativeLogicalBorderWidthCandidates,
    style: &mut NativeLogicalBorderStyleCandidates,
    color: &mut NativeLogicalBorderColorCandidates,
) {
    for index in 0..LOGICAL_BORDER_SIDES {
        let order = border_cascade_order(rule_order, declarations.border_order[index], inline);
        let important = declarations.border_important[index];
        let border = declarations.border[index];
        apply_paint_cascade_declaration(
            border.and_then(project_border_width_declaration),
            specificity,
            order,
            inline,
            important,
            &mut width[index],
        );
        apply_paint_cascade_declaration(
            border.and_then(project_border_style_declaration),
            specificity,
            order,
            inline,
            important,
            &mut style[index],
        );
        apply_paint_cascade_declaration(
            border.and_then(project_border_color_declaration),
            specificity,
            order,
            inline,
            important,
            &mut color[index],
        );
    }
}

fn apply_logical_border_color_cascade(
    declarations: &NativeLogicalBorderDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativeLogicalBorderColorCandidates,
) {
    for (index, declaration) in declarations.color.iter().copied().enumerate() {
        apply_paint_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declarations.color_order[index], inline),
            inline,
            declarations.color_important[index],
            &mut candidates[index],
        );
    }
}

fn apply_logical_border_width_cascade(
    declarations: &NativeLogicalBorderDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativeLogicalBorderWidthCandidates,
) {
    for (index, declaration) in declarations.width.iter().copied().enumerate() {
        apply_paint_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declarations.width_order[index], inline),
            inline,
            declarations.width_important[index],
            &mut candidates[index],
        );
    }
}

fn apply_logical_border_style_cascade(
    declarations: &NativeLogicalBorderDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    candidates: &mut NativeLogicalBorderStyleCandidates,
) {
    for (index, declaration) in declarations.style.iter().copied().enumerate() {
        apply_paint_cascade_declaration(
            declaration,
            specificity,
            border_cascade_order(rule_order, declarations.style_order[index], inline),
            inline,
            declarations.style_important[index],
            &mut candidates[index],
        );
    }
}

fn apply_logical_border_cascade(
    declarations: &NativeLogicalBorderDeclarations,
    specificity: u16,
    rule_order: usize,
    inline: bool,
    targets: NativeLogicalBorderCandidateTargets<'_>,
) {
    apply_logical_cascade_edges(
        &declarations.border,
        &declarations.border_order,
        specificity,
        rule_order,
        inline,
        &mut *targets.border,
    );
    apply_logical_border_shorthand_cascade(
        declarations,
        specificity,
        rule_order,
        inline,
        &mut *targets.width,
        &mut *targets.style,
        &mut *targets.color,
    );
    apply_logical_border_width_cascade(
        declarations,
        specificity,
        rule_order,
        inline,
        &mut *targets.width,
    );
    apply_logical_border_style_cascade(
        declarations,
        specificity,
        rule_order,
        inline,
        &mut *targets.style,
    );
    apply_logical_border_color_cascade(
        declarations,
        specificity,
        rule_order,
        inline,
        &mut *targets.color,
    );
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeFlexDeclarationImportance {
    justify_content: bool,
    align_items: bool,
    align_self: bool,
    align_content: bool,
    flex_direction: bool,
    flex_wrap: bool,
    order: bool,
    flex_grow: bool,
    flex_shrink: bool,
    flex_basis: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeLocalDeclarationImportance {
    display: bool,
    visibility: bool,
    opacity: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeDimensionDeclarationImportance {
    width: bool,
    height: bool,
    min_width: bool,
    max_width: bool,
    min_height: bool,
    max_height: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeBoxModelDeclarationImportance {
    padding: [bool; 4],
    margin: [bool; 4],
    box_sizing: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeOverflowDeclarationImportance {
    shorthand: bool,
    x: bool,
    y: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeTextDeclarationImportance {
    white_space: bool,
    text_align: bool,
    text_align_last: bool,
    text_justify: bool,
    direction: bool,
    text_decoration: bool,
    text_decoration_style: bool,
    text_decoration_skip_ink: bool,
    text_decoration_skip_spaces: bool,
    text_decoration_thickness: bool,
    text_underline_offset: bool,
    text_transform: bool,
    font_weight: bool,
    font_style: bool,
    word_break: bool,
    text_overflow: bool,
    vertical_align: bool,
    text_indent: bool,
    word_spacing: bool,
    letter_spacing: bool,
    line_height: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeDeclarations {
    display: Option<LocalCascadeDeclaration<DisplayValue>>,
    visibility: Option<LocalCascadeDeclaration<VisibilityValue>>,
    opacity: Option<LocalCascadeDeclaration<u8>>,
    local_importance: NativeLocalDeclarationImportance,
    dimension_importance: NativeDimensionDeclarationImportance,
    box_model_importance: NativeBoxModelDeclarationImportance,
    flex_importance: NativeFlexDeclarationImportance,
    text_importance: NativeTextDeclarationImportance,
    white_space: Option<InheritedTextDeclaration<WhiteSpaceValue>>,
    text_align: Option<InheritedTextDeclaration<TextAlignValue>>,
    text_align_last: Option<InheritedTextDeclaration<TextAlignLastValue>>,
    text_justify: Option<InheritedTextDeclaration<TextJustifyValue>>,
    justify_content: Option<JustifyContentDeclaration>,
    align_items: Option<AlignItemsDeclaration>,
    align_self: Option<AlignSelfDeclaration>,
    align_content: Option<AlignContentDeclaration>,
    flex_direction: Option<FlexDirectionDeclaration>,
    direction: Option<InheritedTextDeclaration<DirectionValue>>,
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
    text_decoration_color_important: bool,
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
    gap_important: bool,
    row_gap: Option<GapComponentDeclaration>,
    row_gap_order: usize,
    row_gap_important: bool,
    column_gap: Option<GapComponentDeclaration>,
    column_gap_order: usize,
    column_gap_important: bool,
    width: Option<LocalCascadeDeclaration<u32>>,
    height: Option<LocalCascadeDeclaration<u32>>,
    min_width: Option<LocalCascadeDeclaration<u32>>,
    max_width: Option<LocalCascadeDeclaration<u32>>,
    min_height: Option<LocalCascadeDeclaration<u32>>,
    max_height: Option<LocalCascadeDeclaration<u32>>,
    line_height: Option<InheritedTextDeclaration<u32>>,
    background_color: Option<LocalCascadeDeclaration<NativeBackgroundColorValue>>,
    background_color_important: bool,
    border: [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; 4],
    border_order: [usize; 4],
    border_important: [bool; 4],
    logical_border: NativeLogicalBorderDeclarations,
    border_width: [Option<LocalCascadeDeclaration<NativeBorderWidthValue>>; 4],
    border_width_order: [usize; 4],
    border_width_important: [bool; 4],
    border_style: [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; 4],
    border_style_order: [usize; 4],
    border_style_important: [bool; 4],
    border_color: [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; 4],
    border_color_order: [usize; 4],
    border_color_important: [bool; 4],
    border_radius: Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
    border_radius_order: usize,
    border_radius_important: bool,
    border_radius_corners: [Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>; 4],
    border_radius_corner_orders: [usize; 4],
    border_radius_corner_important: [bool; 4],
    logical_border_radius: NativeLogicalBorderRadiusDeclarations,
    padding: [Option<LocalCascadeDeclaration<u32>>; 4],
    padding_order: [usize; 4],
    margin: [Option<LocalCascadeDeclaration<NativeMarginValue>>; 4],
    margin_order: [usize; 4],
    box_sizing: Option<LocalCascadeDeclaration<NativeBoxSizing>>,
    color: Option<LocalCascadeDeclaration<NativeColorValue>>,
    color_important: bool,
    overflow: Option<LocalCascadeDeclaration<OverflowValue>>,
    overflow_x: Option<LocalCascadeDeclaration<OverflowValue>>,
    overflow_y: Option<LocalCascadeDeclaration<OverflowValue>>,
    overflow_importance: NativeOverflowDeclarationImportance,
    logical_box_model: NativeLogicalBoxModelDeclarations,
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

fn strip_important_suffix(value: &str) -> (&str, bool) {
    let value = value.trim();
    let Some((base, suffix)) = value.rsplit_once('!') else {
        return (value, false);
    };
    if !base.trim().is_empty() && suffix.eq_ignore_ascii_case("important") {
        (base.trim_end(), true)
    } else {
        (value, false)
    }
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
        let (value, _) = strip_important_suffix(value);
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
            "background-color" => parse_background_color_declaration(value).is_some(),
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
            | "border-left-width" => parse_border_width_side_declaration(value).is_some(),
            "border-block-width" | "border-inline-width" => {
                parse_logical_border_width_pair(value).is_some()
            }
            "border-block-start-width"
            | "border-block-end-width"
            | "border-inline-start-width"
            | "border-inline-end-width" => parse_border_width_side_declaration(value).is_some(),
            "border-style" => parse_border_style_declaration(value).is_some(),
            "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style" => parse_border_style_side_declaration(value).is_some(),
            "border-block-style" | "border-inline-style" => {
                parse_logical_border_style_pair(value).is_some()
            }
            "border-block-start-style"
            | "border-block-end-style"
            | "border-inline-start-style"
            | "border-inline-end-style" => parse_border_style_side_declaration(value).is_some(),
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
        let (value, important) = strip_important_suffix(value);
        match property.to_ascii_lowercase().as_str() {
            "display" => {
                if let Some(parsed) = parse_display_declaration(value) {
                    declarations.display = Some(parsed);
                    declarations.local_importance.display = important;
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
            "background-color" => {
                if let Some(parsed) = parse_background_color_declaration(value) {
                    declarations.background_color = Some(parsed);
                    declarations.background_color_important = important;
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
                    parse_border_width_side_declaration(value),
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
                    parse_border_width_side_declaration(value),
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
                    parse_border_width_side_declaration(value),
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
                    parse_border_width_side_declaration(value),
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
                    parse_border_style_side_declaration(value),
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
                    parse_border_style_side_declaration(value),
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
                    parse_border_style_side_declaration(value),
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
                    parse_border_style_side_declaration(value),
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
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[0] = Some(value);
                    declarations.padding_order[0] = declaration_order;
                    declarations.box_model_importance.padding[0] = important;
                }
            }
            "padding-right" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[1] = Some(value);
                    declarations.padding_order[1] = declaration_order;
                    declarations.box_model_importance.padding[1] = important;
                }
            }
            "padding-bottom" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[2] = Some(value);
                    declarations.padding_order[2] = declaration_order;
                    declarations.box_model_importance.padding[2] = important;
                }
            }
            "padding-left" => {
                if let Some(value) = parse_local_padding_declaration(value) {
                    declarations.padding[3] = Some(value);
                    declarations.padding_order[3] = declaration_order;
                    declarations.box_model_importance.padding[3] = important;
                }
            }
            "margin-top" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[0] = Some(value);
                    declarations.margin_order[0] = declaration_order;
                    declarations.box_model_importance.margin[0] = important;
                }
            }
            "margin-right" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[1] = Some(value);
                    declarations.margin_order[1] = declaration_order;
                    declarations.box_model_importance.margin[1] = important;
                }
            }
            "margin-bottom" => {
                if let Some(value) = parse_local_margin_declaration(value) {
                    declarations.margin[2] = Some(value);
                    declarations.margin_order[2] = declaration_order;
                    declarations.box_model_importance.margin[2] = important;
                }
            }
            "margin-left" => {
                if let Some(value) = parse_local_margin_declaration(value) {
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

#[cfg(test)]
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

fn parse_complete_border(value: &str) -> Option<NativeBorderDeclaration> {
    let mut parts = value.split_ascii_whitespace();
    let width = parse_dimension(parts.next()?)?;
    let style = parts.next()?;
    let color = parts.collect::<Vec<_>>().join(" ");
    let color = parse_border_color_value(&color)?;
    match (parse_border_style_value(style)?, color) {
        (NativeBorderStyleValue::Paint(style), NativeBorderColorValue::Color(color)) => {
            Some(NativeBorderDeclaration::Complete(NativeBorderSide {
                width,
                style,
                color,
            }))
        }
        (NativeBorderStyleValue::Paint(style), NativeBorderColorValue::CurrentColor) => {
            Some(NativeBorderDeclaration::CompleteCurrentColor { width, style })
        }
        (NativeBorderStyleValue::None, NativeBorderColorValue::Color(color)) => {
            Some(NativeBorderDeclaration::CompleteNone { width, color })
        }
        (NativeBorderStyleValue::None, NativeBorderColorValue::CurrentColor) => {
            Some(NativeBorderDeclaration::CompleteNoneCurrentColor { width })
        }
        (NativeBorderStyleValue::Hidden, NativeBorderColorValue::Color(color)) => {
            Some(NativeBorderDeclaration::CompleteHidden { width, color })
        }
        (NativeBorderStyleValue::Hidden, NativeBorderColorValue::CurrentColor) => {
            Some(NativeBorderDeclaration::CompleteHiddenCurrentColor { width })
        }
        _ => None,
    }
}

fn parse_border_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderDeclaration>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    if value.eq_ignore_ascii_case("none") {
        return Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None,
        ));
    }
    if value.eq_ignore_ascii_case("hidden") {
        return Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Hidden,
        ));
    }
    for (keyword, declaration) in [
        ("inherit", NativeBorderDeclaration::Inherit),
        ("unset", NativeBorderDeclaration::Unset),
        ("initial", NativeBorderDeclaration::Initial),
        ("revert", NativeBorderDeclaration::Revert),
    ] {
        if value.eq_ignore_ascii_case(keyword) {
            return Some(LocalCascadeDeclaration::Value(declaration));
        }
    }
    parse_complete_border(value).map(LocalCascadeDeclaration::Value)
}

fn parse_border_width_declaration(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderWidthValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_width_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    expand_box_edges(&values).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_border_width_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderWidthValue>> {
    parse_local_cascade_declaration(value, parse_border_width_value)
}

fn parse_border_width_value(value: &str) -> Option<NativeBorderWidthValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderWidthValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderWidthValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderWidthValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderWidthValue::Revert)
    } else {
        parse_dimension(value).map(NativeBorderWidthValue::Width)
    }
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
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
    expand_box_edges(&values).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_border_style_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderStyleValue>> {
    parse_local_cascade_declaration(value, parse_border_style_value)
}

fn parse_border_color(value: &str) -> Option<[NativeBorderColorValue; 4]> {
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_border_color_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().any(|value| value.is_css_wide()) {
        return None;
    }
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
) -> Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 4]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 4]);
    }
    parse_border_color(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_border_color_side_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderColorValue>> {
    parse_local_cascade_declaration(value, parse_border_color_value)
}

fn parse_logical_component_pair<T: Copy>(
    value: &str,
    parse_value: impl Fn(&str) -> Option<T>,
    is_css_wide: impl Fn(T) -> bool,
) -> Option<[LocalCascadeDeclaration<T>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    let values = split_css_value_tokens(value)?
        .into_iter()
        .map(parse_value)
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 1 && values.iter().copied().any(is_css_wide) {
        return None;
    }
    match values.as_slice() {
        [value] => Some([LocalCascadeDeclaration::Value(*value); 2]),
        [start, end] => Some([
            LocalCascadeDeclaration::Value(*start),
            LocalCascadeDeclaration::Value(*end),
        ]),
        _ => None,
    }
}

fn parse_logical_border_width_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderWidthValue>; 2]> {
    parse_logical_component_pair(
        value,
        parse_border_width_value,
        NativeBorderWidthValue::is_css_wide,
    )
}

fn parse_logical_border_style_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderStyleValue>; 2]> {
    parse_logical_component_pair(
        value,
        parse_border_style_value,
        NativeBorderStyleValue::is_css_wide,
    )
}

fn parse_logical_border_color_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 2]> {
    parse_logical_component_pair(
        value,
        parse_border_color_value,
        NativeBorderColorValue::is_css_wide,
    )
}

fn parse_border_color_value(value: &str) -> Option<NativeBorderColorValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("currentColor") {
        Some(NativeBorderColorValue::CurrentColor)
    } else if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderColorValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderColorValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderColorValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderColorValue::Revert)
    } else {
        parse_color(value).map(NativeBorderColorValue::Color)
    }
}

impl NativeBorderColorValue {
    const fn is_css_wide(self) -> bool {
        matches!(
            self,
            Self::Inherit | Self::Unset | Self::Initial | Self::Revert
        )
    }
}

fn parse_background_color_value(value: &str) -> Option<NativeBackgroundColorValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("currentColor") {
        Some(NativeBackgroundColorValue::CurrentColor)
    } else if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBackgroundColorValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBackgroundColorValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBackgroundColorValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBackgroundColorValue::Revert)
    } else {
        parse_color(value).map(NativeBackgroundColorValue::Color)
    }
}

fn parse_background_color_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBackgroundColorValue>> {
    parse_local_cascade_declaration(value, parse_background_color_value)
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
) -> Option<LocalCascadeDeclaration<NativeBorderRadiusValue>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    let css_wide = if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderRadiusValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderRadiusValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderRadiusValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderRadiusValue::Revert)
    } else {
        None
    };
    css_wide
        .or_else(|| parse_border_radius(value).map(NativeBorderRadiusValue::Radius))
        .map(LocalCascadeDeclaration::Value)
}

fn parse_border_radius_corner_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBorderRadiusValue>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    let css_wide = if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderRadiusValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderRadiusValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderRadiusValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderRadiusValue::Revert)
    } else {
        None
    };
    css_wide
        .or_else(|| parse_dimension(value).map(NativeBorderRadiusValue::Corner))
        .map(LocalCascadeDeclaration::Value)
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
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 4]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(0); 4]);
    }
    parse_box_edges(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_box_edge_pair(value: &str) -> Option<[u32; 2]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_dimension)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some([*all; 2]),
        [start, end] => Some([*start, *end]),
        _ => None,
    }
}

fn parse_local_box_edge_pair(value: &str) -> Option<[LocalCascadeDeclaration<u32>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 2]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(0); 2]);
    }
    parse_box_edge_pair(value).map(|values| values.map(LocalCascadeDeclaration::Value))
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
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 4]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)); 4]);
    }
    parse_margin_edges(value).map(|values| values.map(LocalCascadeDeclaration::Value))
}

fn parse_margin_edge_pair(value: &str) -> Option<[NativeMarginValue; 2]> {
    let values = value
        .split_ascii_whitespace()
        .map(parse_margin_value)
        .collect::<Option<Vec<_>>>()?;
    match values.as_slice() {
        [all] => Some([*all; 2]),
        [start, end] => Some([*start, *end]),
        _ => None,
    }
}

fn parse_local_margin_edge_pair(
    value: &str,
) -> Option<[LocalCascadeDeclaration<NativeMarginValue>; 2]> {
    if value.trim().eq_ignore_ascii_case("revert-layer") {
        return Some([LocalCascadeDeclaration::RevertLayer; 2]);
    }
    if is_inherit_keyword(value) {
        return Some([LocalCascadeDeclaration::Inherit; 2]);
    }
    if is_local_reset_keyword(value) {
        return Some([LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)); 2]);
    }
    parse_margin_edge_pair(value).map(|values| values.map(LocalCascadeDeclaration::Value))
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
    if value.eq_ignore_ascii_case("inherit") {
        Some(NativeBorderStyleValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeBorderStyleValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeBorderStyleValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeBorderStyleValue::Revert)
    } else if value.eq_ignore_ascii_case("none") {
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
        "inherit" => Some(NativeTextDecorationStyleDeclaration::Inherit),
        "initial" => Some(NativeTextDecorationStyleDeclaration::Initial),
        "unset" => Some(NativeTextDecorationStyleDeclaration::Unset),
        "revert" => Some(NativeTextDecorationStyleDeclaration::Revert),
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
        "inherit" => Some(NativeTextDecorationSkipInkDeclaration::Inherit),
        "initial" => Some(NativeTextDecorationSkipInkDeclaration::Initial),
        "unset" => Some(NativeTextDecorationSkipInkDeclaration::Unset),
        "revert" => Some(NativeTextDecorationSkipInkDeclaration::Revert),
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
    if value.eq_ignore_ascii_case("inherit") {
        return Some(NativeTextDecorationThicknessDeclaration::Inherit);
    }
    if value.eq_ignore_ascii_case("initial") {
        return Some(NativeTextDecorationThicknessDeclaration::Initial);
    }
    if value.eq_ignore_ascii_case("unset") {
        return Some(NativeTextDecorationThicknessDeclaration::Unset);
    }
    if value.eq_ignore_ascii_case("revert") {
        return Some(NativeTextDecorationThicknessDeclaration::Revert);
    }
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationThicknessDeclaration::RevertLayer);
    }
    parse_dimension(value)
        .filter(|value| (1..=MAX_NATIVE_TEXT_DECORATION_THICKNESS).contains(value))
        .map(NativeTextDecorationThicknessDeclaration::Value)
}

fn parse_text_underline_offset(value: &str) -> Option<NativeTextUnderlineOffsetDeclaration> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "inherit" => return Some(NativeTextUnderlineOffsetDeclaration::Inherit),
        "initial" => return Some(NativeTextUnderlineOffsetDeclaration::Initial),
        "unset" => return Some(NativeTextUnderlineOffsetDeclaration::Unset),
        "revert" => return Some(NativeTextUnderlineOffsetDeclaration::Revert),
        "revert-layer" => return Some(NativeTextUnderlineOffsetDeclaration::RevertLayer),
        _ => {}
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
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(NativeTextDecorationColorDeclaration::RevertLayer);
    }
    if value.eq_ignore_ascii_case("currentColor") {
        return Some(NativeTextDecorationColorDeclaration::CurrentColor);
    }
    for (keyword, declaration) in [
        ("inherit", NativeTextDecorationColorDeclaration::Inherit),
        ("unset", NativeTextDecorationColorDeclaration::Unset),
        ("initial", NativeTextDecorationColorDeclaration::Initial),
        ("revert", NativeTextDecorationColorDeclaration::Revert),
    ] {
        if value.eq_ignore_ascii_case(keyword) {
            return Some(declaration);
        }
    }
    parse_color(value).map(NativeTextDecorationColorDeclaration::Value)
}

fn parse_local_color_declaration(value: &str) -> Option<LocalCascadeDeclaration<NativeColorValue>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    let keyword = if value.eq_ignore_ascii_case("currentColor") {
        Some(NativeColorValue::CurrentColor)
    } else if value.eq_ignore_ascii_case("inherit") {
        Some(NativeColorValue::Inherit)
    } else if value.eq_ignore_ascii_case("unset") {
        Some(NativeColorValue::Unset)
    } else if value.eq_ignore_ascii_case("initial") {
        Some(NativeColorValue::Initial)
    } else if value.eq_ignore_ascii_case("revert") {
        Some(NativeColorValue::Revert)
    } else {
        None
    };
    keyword
        .or_else(|| parse_color(value).map(NativeColorValue::Color))
        .map(LocalCascadeDeclaration::Value)
}

fn set_border_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(border) = parse_border_declaration(value) {
        sides[index] = Some(border);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_border_radius_corner(
    corners: &mut [Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(radius) = parse_border_radius_corner_declaration(value) {
        corners[index] = Some(radius);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_logical_border_pair<T: Copy>(
    sides: &mut [Option<LocalCascadeDeclaration<T>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<T>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

fn set_logical_border_width_pair(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderWidthValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<NativeBorderWidthValue>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

fn set_logical_border_width_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderWidthValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<NativeBorderWidthValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_logical_border_style_pair(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<NativeBorderStyleValue>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

fn set_logical_border_style_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<NativeBorderStyleValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_logical_border_side<T: Copy>(
    sides: &mut [Option<LocalCascadeDeclaration<T>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<T>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_logical_border_color_pair(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    indices: [usize; 2],
    values: Option<[LocalCascadeDeclaration<NativeBorderColorValue>; 2]>,
    declaration_order: usize,
    important: bool,
) {
    let Some([start_value, end_value]) = values else {
        return;
    };
    let [start, end] = indices;
    sides[start] = Some(start_value);
    sides[end] = Some(end_value);
    orders[start] = declaration_order;
    orders[end] = declaration_order;
    important_flags[start] = important;
    important_flags[end] = important;
}

fn set_logical_border_color_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<NativeBorderColorValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        sides[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_logical_border_radius_corner(
    corners: &mut [Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>; LOGICAL_BORDER_SIDES],
    orders: &mut [usize; LOGICAL_BORDER_SIDES],
    important_flags: &mut [bool; LOGICAL_BORDER_SIDES],
    index: usize,
    value: Option<LocalCascadeDeclaration<NativeBorderRadiusValue>>,
    declaration_order: usize,
    important: bool,
) {
    if let Some(value) = value {
        corners[index] = Some(value);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_border_color_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderColorValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(color) = parse_border_color_side_declaration(value) {
        sides[index] = Some(color);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_border_width_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderWidthValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(width) = parse_border_width_side_declaration(value) {
        sides[index] = Some(width);
        orders[index] = declaration_order;
        important_flags[index] = important;
    }
}

fn set_border_style_side(
    sides: &mut [Option<LocalCascadeDeclaration<NativeBorderStyleValue>>; 4],
    orders: &mut [usize; 4],
    important_flags: &mut [bool; 4],
    index: usize,
    value: &str,
    declaration_order: usize,
    important: bool,
) {
    if let Some(style) = parse_border_style_side_declaration(value) {
        sides[index] = Some(style);
        orders[index] = declaration_order;
        important_flags[index] = important;
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

fn parse_line_height_declaration(value: &str) -> Option<InheritedTextDeclaration<u32>> {
    parse_inherited_text_declaration(value, parse_line_height)
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

fn parse_white_space_declaration(value: &str) -> Option<InheritedTextDeclaration<WhiteSpaceValue>> {
    parse_inherited_text_declaration(value, parse_white_space)
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

fn parse_text_align_declaration(value: &str) -> Option<InheritedTextDeclaration<TextAlignValue>> {
    parse_inherited_text_declaration(value, parse_text_align)
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

fn parse_text_align_last_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextAlignLastValue>> {
    parse_inherited_text_declaration(value, parse_text_align_last)
}

fn parse_text_justify(value: &str) -> Option<TextJustifyValue> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(TextJustifyValue::Auto),
        "none" => Some(TextJustifyValue::None),
        "inter-word" => Some(TextJustifyValue::InterWord),
        _ => None,
    }
}

fn parse_text_justify_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<TextJustifyValue>> {
    parse_inherited_text_declaration(value, parse_text_justify)
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

fn parse_direction_declaration(value: &str) -> Option<InheritedTextDeclaration<DirectionValue>> {
    parse_inherited_text_declaration(value, parse_direction)
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
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Some(NativeTextDecorationDeclaration::Inherit),
        "initial" => Some(NativeTextDecorationDeclaration::Initial),
        "unset" => Some(NativeTextDecorationDeclaration::Unset),
        "revert" => Some(NativeTextDecorationDeclaration::Revert),
        "revert-layer" => Some(NativeTextDecorationDeclaration::RevertLayer),
        _ => parse_text_decoration(value).map(NativeTextDecorationDeclaration::Value),
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

fn parse_inherited_text_declaration<T: Copy>(
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
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Reset);
    }
    parse_local_cascade_declaration(value, parse_dimension)
}

fn parse_local_padding_declaration(value: &str) -> Option<LocalCascadeDeclaration<u32>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(0));
    }
    parse_local_cascade_declaration(value, parse_dimension)
}

fn parse_local_margin_declaration(
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

fn parse_local_box_sizing_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<NativeBoxSizing>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Value(NativeBoxSizing::ContentBox));
    }
    parse_local_cascade_declaration(value, parse_box_sizing)
}

fn is_local_reset_keyword(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("initial")
        || value.eq_ignore_ascii_case("unset")
        || value.eq_ignore_ascii_case("revert")
}

fn is_inherit_keyword(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("inherit")
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

    fn border_color_value(color: NativeColor) -> NativeBorderColorValue {
        NativeBorderColorValue::Color(color)
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
            Some(InheritedTextDeclaration::Value(WhiteSpaceValue::PreLine))
        );
        assert_eq!(
            declarations.text_align,
            Some(InheritedTextDeclaration::Value(TextAlignValue::Center))
        );
        assert_eq!(
            declarations.text_align_last,
            Some(InheritedTextDeclaration::Value(TextAlignLastValue::End))
        );
        assert_eq!(
            declarations.text_justify,
            Some(InheritedTextDeclaration::Value(TextJustifyValue::InterWord))
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
            Some(InheritedTextDeclaration::Value(DirectionValue::Rtl))
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
            Some(InheritedTextDeclaration::Value(28))
        );
        assert_eq!(
            declarations.color,
            Some(LocalCascadeDeclaration::Value(NativeColorValue::Color(
                NativeColor::RED
            )))
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
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Radius(NativeBorderRadius {
                    top_left: 1,
                    top_right: 2,
                    bottom_right: 3,
                    bottom_left: 4,
                }),
            ))
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
    fn local_paint_color_parser_accepts_css_wide_keywords_and_preserves_valid_values() {
        assert_eq!(
            parse_local_color_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_local_color_declaration("rgba(1, 2, 3, 0.5)"),
            Some(LocalCascadeDeclaration::Value(NativeColorValue::Color(
                NativeColor {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 128,
                }
            )))
        );
        for value in [
            "revert-layer red",
            "red revert-layer",
            "linear-gradient(red, blue)",
            "color(display-p3 1 0 0)",
        ] {
            assert_eq!(parse_local_color_declaration(value), None, "value={value}");
        }
        assert_eq!(
            parse_local_color_declaration("CuRrEnTcOlOr"),
            Some(LocalCascadeDeclaration::Value(
                NativeColorValue::CurrentColor
            ))
        );
        for (value, expected) in [
            ("InHeRiT", NativeColorValue::Inherit),
            ("UnSeT", NativeColorValue::Unset),
            ("InItIaL", NativeColorValue::Initial),
            ("ReVeRt", NativeColorValue::Revert),
        ] {
            assert_eq!(
                parse_local_color_declaration(value),
                Some(LocalCascadeDeclaration::Value(expected)),
                "value={value}"
            );
        }

        let document = NativeDocument::parse(
            "<style>#parent { color:green; } #current { color:currentColor; } #inherit { color:inherit; } #unset { color:unset; } #revert { color:revert; } #initial { color:initial; } #root { color:currentColor; }</style><div id='parent'><span id='current'>Current</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='initial'>Initial</span></div><div id='root'>Root</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let parent = document.resolve_target("id=parent").unwrap();
        let current = document.resolve_target("id=current").unwrap();
        let inherit = document.resolve_target("id=inherit").unwrap();
        let unset = document.resolve_target("id=unset").unwrap();
        let revert = document.resolve_target("id=revert").unwrap();
        let initial = document.resolve_target("id=initial").unwrap();
        let root = document.resolve_target("id=root").unwrap();
        let omitted_document = NativeDocument::parse(
            "<div id='omitted'>Omitted</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let omitted = omitted_document.resolve_target("id=omitted").unwrap();
        assert_eq!(
            document.computed_style_for_layout(parent).color(),
            Some(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: u8::MAX,
            })
        );
        assert_eq!(
            document.computed_style_for_layout(current).color(),
            Some(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: u8::MAX,
            })
        );
        for node_id in [inherit, unset, revert] {
            assert_eq!(
                document.computed_style_for_layout(node_id).color(),
                Some(NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: u8::MAX,
                })
            );
        }
        assert_eq!(
            document.computed_style_for_layout(initial).color(),
            Some(NativeColor::BLACK)
        );
        assert_eq!(
            document.computed_style_for_layout(root).color(),
            Some(NativeColor::BLACK)
        );
        assert_eq!(
            NativeStylesheet::default()
                .computed_for(omitted_document.node(omitted).unwrap())
                .color(),
            None
        );

        let declarations = parse_declarations(
            "background-color: red; background-color: currentColor; color: blue; color: inherit",
        );
        assert_eq!(
            declarations.background_color,
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::CurrentColor
            ))
        );
        assert_eq!(
            declarations.color,
            Some(LocalCascadeDeclaration::Value(NativeColorValue::Inherit))
        );
        assert_eq!(
            parse_declarations("background-color: revert-layer").background_color,
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_background_color_declaration("CuRrEnTcOlOr"),
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::CurrentColor
            ))
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
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Radius(NativeBorderRadius {
                    top_left: 1,
                    top_right: 2,
                    bottom_right: 1,
                    bottom_left: 2,
                }),
            ))
        );
        assert_eq!(parse_border_radius("inherit"), None);
        assert_eq!(
            parse_border_radius_declaration("InHeRiT"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Inherit
            ))
        );
        assert_eq!(
            parse_border_radius_declaration("UNSET"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Unset
            ))
        );
        assert_eq!(
            parse_border_radius_declaration("initial"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Initial
            ))
        );
        assert_eq!(
            parse_border_radius_declaration("revert"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Revert
            ))
        );
        for value in [
            "inherit 1px",
            "1px inherit",
            "initial 2px",
            "1px revert",
            "revert-layer 1px",
        ] {
            assert_eq!(
                parse_border_radius_declaration(value),
                None,
                "value={value}"
            );
        }
        assert_eq!(parse_border_radius_declaration("1px revert-layer"), None);
    }

    #[test]
    fn border_radius_corner_parser_preserves_bounded_values_and_order() {
        assert_eq!(
            parse_border_radius_corner_declaration("9px"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Corner(9),
            ))
        );
        for (value, expected) in [
            (
                "InHeRiT",
                LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Inherit),
            ),
            (
                "UNSET",
                LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Unset),
            ),
            (
                "initial",
                LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Initial),
            ),
            (
                "ReVeRt",
                LocalCascadeDeclaration::Value(NativeBorderRadiusValue::Revert),
            ),
            ("revert-layer", LocalCascadeDeclaration::RevertLayer),
        ] {
            assert_eq!(
                parse_border_radius_corner_declaration(value),
                Some(expected)
            );
        }
        for value in [
            "1px 2px",
            "50%",
            "-1px",
            "1.5px",
            "20000px",
            "inherit 1px",
            "1px revert-layer",
        ] {
            assert_eq!(
                parse_border_radius_corner_declaration(value),
                None,
                "value={value}"
            );
        }

        let declarations = parse_declarations(
            "border-radius: 1px 2px 3px 4px; border-top-left-radius: 9px; border-bottom-left-radius: 10px;",
        );
        assert_eq!(declarations.border_radius_order, 0);
        assert_eq!(
            declarations.border_radius_corners,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderRadiusValue::Corner(9)
                )),
                None,
                None,
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderRadiusValue::Corner(10)
                )),
            ]
        );
        assert_eq!(declarations.border_radius_corner_orders, [1, 0, 0, 2]);
    }

    #[test]
    fn stylesheet_cascade_composes_border_radius_shorthand_and_corners() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#all { border-radius: 1px 2px 3px 4px; border-top-left-radius: 9px; border-bottom-right-radius: 8px; } #reverse { border-top-left-radius: 9px; border-radius: 1px 2px 3px 4px; } #invalid { border-radius: 5px 6px 7px 8px; border-top-right-radius: 50%; } #inherit { border-radius: 1px; border-top-left-radius: inherit; }".into(),
        ])
        .unwrap();
        let inherited_radius = NativeBorderRadius {
            top_left: 11,
            top_right: 12,
            bottom_right: 13,
            bottom_left: 14,
        };
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Radius</div>"));
            stylesheet
                .computed_for_with_matcher(
                    &element,
                    NativeInheritedStyle {
                        border_radius: inherited_radius,
                        ..NativeInheritedStyle::default()
                    },
                    |selector| selector.matches(&element),
                )
                .border_radius()
        };
        assert_eq!(
            computed("all"),
            NativeBorderRadius {
                top_left: 9,
                top_right: 2,
                bottom_right: 8,
                bottom_left: 4,
            }
        );
        assert_eq!(
            computed("reverse"),
            NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 4,
            }
        );
        assert_eq!(
            computed("invalid"),
            NativeBorderRadius {
                top_left: 5,
                top_right: 6,
                bottom_right: 7,
                bottom_left: 8,
            }
        );
        assert_eq!(
            computed("inherit"),
            NativeBorderRadius {
                top_left: 11,
                top_right: 1,
                bottom_right: 1,
                bottom_left: 1,
            }
        );
    }

    #[test]
    fn border_radius_corner_revert_layer_rolls_back_only_selected_corner() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #card { border-radius: 1px 2px 3px 4px; } } @layer theme { #card { border-radius: 9px; border-top-left-radius: revert-layer; border-bottom-right-radius: 7px; } }".into(),
        ])
        .unwrap();
        assert_eq!(
            stylesheet
                .computed_for(&node("<div id='card'>Radius</div>"))
                .border_radius(),
            NativeBorderRadius {
                top_left: 1,
                top_right: 9,
                bottom_right: 7,
                bottom_left: 9,
            }
        );
    }

    #[test]
    fn logical_border_radius_parser_preserves_corner_values_and_order() {
        let declarations = parse_declarations(
            "border-start-start-radius: 9px; border-start-end-radius: inherit; border-end-start-radius: revert-layer; border-end-end-radius: 10px;",
        );
        assert_eq!(
            declarations.logical_border_radius.corners,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderRadiusValue::Corner(9)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderRadiusValue::Inherit
                )),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderRadiusValue::Corner(10)
                )),
            ]
        );
        assert_eq!(
            declarations.logical_border_radius.corner_orders,
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn stylesheet_cascade_maps_logical_border_radius_for_ltr_and_rtl() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#ltr { direction: ltr; border-start-start-radius: 1px; border-start-end-radius: 2px; border-end-start-radius: 4px; border-end-end-radius: 3px; } #rtl { direction: rtl; border-start-start-radius: 1px; border-start-end-radius: 2px; border-end-start-radius: 4px; border-end-end-radius: 3px; } #precedence { border-top-left-radius: 9px; border-start-start-radius: 7px; } #reverse { border-start-start-radius: 7px; border-top-left-radius: 9px; } #invalid { border-radius: 5px; border-start-start-radius: 50%; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Radius</div>"));
            stylesheet
                .computed_for_with_matcher(&element, NativeInheritedStyle::default(), |selector| {
                    selector.matches(&element)
                })
                .border_radius()
        };
        assert_eq!(
            computed("ltr"),
            NativeBorderRadius {
                top_left: 1,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 4,
            }
        );
        assert_eq!(
            computed("rtl"),
            NativeBorderRadius {
                top_left: 2,
                top_right: 1,
                bottom_right: 4,
                bottom_left: 3,
            }
        );
        assert_eq!(computed("precedence").top_left, 7);
        assert_eq!(computed("reverse").top_left, 9);
        assert_eq!(
            computed("invalid"),
            NativeBorderRadius {
                top_left: 5,
                top_right: 5,
                bottom_right: 5,
                bottom_left: 5,
            }
        );
    }

    #[test]
    fn logical_border_radius_revert_layer_rolls_back_only_mapped_corner() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #card { direction: rtl; border-radius: 1px 2px 3px 4px; } } @layer theme { #card { border-start-start-radius: 9px; border-start-start-radius: revert-layer; border-start-end-radius: 8px; } }".into(),
        ])
        .unwrap();
        assert_eq!(
            stylesheet
                .computed_for(&node("<div id='card'>Radius</div>"))
                .border_radius(),
            NativeBorderRadius {
                top_left: 8,
                top_right: 2,
                bottom_right: 3,
                bottom_left: 4,
            }
        );
    }

    #[test]
    fn border_radius_important_parser_tracks_terminal_case_insensitive_priority() {
        let declarations = parse_declarations(
            "border-radius: 1px !IMPORTANT; border-top-left-radius: 2px; border-top-right-radius: 3px !important; border-start-start-radius: 4px !important; border-end-end-radius: 50% !important;",
        );
        assert_eq!(
            declarations.border_radius,
            Some(LocalCascadeDeclaration::Value(
                NativeBorderRadiusValue::Radius(NativeBorderRadius {
                    top_left: 1,
                    top_right: 1,
                    bottom_right: 1,
                    bottom_left: 1,
                }),
            ))
        );
        assert!(declarations.border_radius_important);
        assert_eq!(
            declarations.border_radius_corner_important,
            [false, true, false, false]
        );
        assert_eq!(
            declarations.logical_border_radius.important,
            [true, false, false, false]
        );
        assert_eq!(strip_important_suffix(" 4px !ImPoRtAnT "), ("4px", true));
        assert_eq!(
            strip_important_suffix("4px ! important"),
            ("4px ! important", false)
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_radius_important_priority_and_revert_layer() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { border-radius: 1px !important; } #rollback { border-radius: 1px !important; border-top-left-radius: revert-layer !important; } #normal { border-radius: 1px; } #mixed { border-radius: 1px !important; } #logical { direction: rtl; border-top-right-radius: 2px !important; border-start-start-radius: 7px !important; } #logical-reverse { direction: rtl; border-start-start-radius: 7px !important; border-top-right-radius: 2px !important; } } @layer theme { #layered { border-radius: 2px !important; } #rollback { border-radius: 2px !important; } #normal { border-radius: 2px; } } #layered { border-radius: 3px !important; } #normal { border-radius: 3px; } #mixed { border-radius: 9px; } #inline { border-radius: 3px !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Radius</div>")))
                .border_radius()
        };
        let uniform = |radius| NativeBorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };

        assert_eq!(computed("layered"), uniform(1));
        assert_eq!(computed("normal"), uniform(3));
        assert_eq!(computed("mixed"), uniform(1));
        assert_eq!(
            computed("rollback"),
            NativeBorderRadius {
                top_left: 2,
                top_right: 1,
                bottom_right: 1,
                bottom_left: 1,
            }
        );
        assert_eq!(computed("logical").top_right, 7);
        assert_eq!(computed("logical-reverse").top_right, 2);

        let inline = node("<div id='inline' style='border-radius: 4px !important'>Inline</div>");
        assert_eq!(stylesheet.computed_for(&inline).border_radius(), uniform(4));
    }

    #[test]
    fn stylesheet_cascade_resolves_border_radius_css_wide_values() {
        let inherited_radius = NativeBorderRadius {
            top_left: 7,
            top_right: 6,
            bottom_right: 5,
            bottom_left: 4,
        };
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border-radius: inherit; } #unset { border-radius: unset; } #initial { border-radius: initial; } #revert { border-radius: revert; } #omitted {} #invalid { border-radius: inherit; border-radius: invalid; } #mixed { border-radius: inherit 1px; }"
                .into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Radius</div>"));
            stylesheet
                .computed_for_with_matcher(
                    &element,
                    NativeInheritedStyle {
                        border_radius: inherited_radius,
                        ..NativeInheritedStyle::default()
                    },
                    |selector| selector.matches(&element),
                )
                .border_radius()
        };

        assert_eq!(computed("inherit"), inherited_radius);
        assert_eq!(computed("invalid"), inherited_radius);
        for id in ["unset", "initial", "revert", "omitted", "mixed"] {
            assert_eq!(computed(id), NativeBorderRadius::default(), "id={id}");
        }
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
    fn local_presentation_important_parser_tracks_markers_and_invalid_preservation() {
        let declarations = parse_declarations(
            "display: none !IMPORTANT; visibility: hidden !important; opacity: 50% !important",
        );
        assert_eq!(
            declarations.display,
            Some(LocalCascadeDeclaration::Value(DisplayValue::None))
        );
        assert_eq!(
            declarations.visibility,
            Some(LocalCascadeDeclaration::Value(VisibilityValue::Hidden))
        );
        assert_eq!(
            declarations.opacity,
            Some(LocalCascadeDeclaration::Value(128))
        );
        assert!(declarations.local_importance.display);
        assert!(declarations.local_importance.visibility);
        assert!(declarations.local_importance.opacity);

        let preserved = parse_declarations(
            "display: none !important; display: invalid !important; visibility: hidden !important; visibility: invalid !important; opacity: 50% !important; opacity: 2 !important",
        );
        assert_eq!(
            preserved.display,
            Some(LocalCascadeDeclaration::Value(DisplayValue::None))
        );
        assert_eq!(
            preserved.visibility,
            Some(LocalCascadeDeclaration::Value(VisibilityValue::Hidden))
        );
        assert_eq!(preserved.opacity, Some(LocalCascadeDeclaration::Value(128)));
        assert!(preserved.local_importance.display);
        assert!(preserved.local_importance.visibility);
        assert!(preserved.local_importance.opacity);

        let normal = parse_declarations(
            "display: none !important; display: block; visibility: hidden !important; visibility: visible; opacity: 50% !important; opacity: 100%",
        );
        assert_eq!(
            normal.display,
            Some(LocalCascadeDeclaration::Value(DisplayValue::Block))
        );
        assert_eq!(
            normal.visibility,
            Some(LocalCascadeDeclaration::Value(VisibilityValue::Other))
        );
        assert_eq!(
            normal.opacity,
            Some(LocalCascadeDeclaration::Value(u8::MAX))
        );
        assert!(!normal.local_importance.display);
        assert!(!normal.local_importance.visibility);
        assert!(!normal.local_importance.opacity);
    }

    #[test]
    fn stylesheet_cascade_resolves_local_presentation_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #important { display:none !important; visibility:hidden !important; opacity:25% !important; } #rollback { display:revert-layer !important; visibility:revert-layer !important; opacity:revert-layer !important; } #invalid { display:none !important; visibility:hidden !important; opacity:25% !important; } #normal { display:none; visibility:hidden; opacity:25%; } } @layer theme { #important { display:block !important; visibility:visible !important; opacity:75% !important; } #rollback { display:block !important; visibility:visible !important; opacity:75% !important; } #invalid { display:unsupported !important; visibility:unsupported !important; opacity:2 !important; } #normal { display:block; visibility:visible; opacity:75%; } } @layer top { #rollback { display:none !important; visibility:hidden !important; opacity:25% !important; } } #important { display:block; visibility:visible; opacity:100%; } #rollback { display:block; visibility:visible; opacity:100%; } #invalid { display:block; visibility:visible; opacity:100%; } #normal { display:block; visibility:visible; opacity:100%; } #unlayered { display:none !important; visibility:hidden !important; opacity:25% !important; } #unlayered { display:block !important; visibility:visible !important; opacity:75% !important; } #inline-rollback { display:none !important; visibility:hidden !important; opacity:25% !important; } #inline-rollback { display:block; visibility:visible; opacity:100%; }"
                .into(),
        ])
        .unwrap();
        let important = node("<div id='important'>Important</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let normal = node("<div id='normal'>Normal</div>");
        let unlayered = node("<div id='unlayered'>Unlayered</div>");
        let inline_rollback = node(
            "<div id='inline-rollback' style='display:ReVeRt-LaYeR !important;visibility:revert-layer !important;opacity:revert-layer !important'>Inline rollback</div>",
        );

        let style_for = |target: &NativeNode| stylesheet.computed_for(target);
        let important_style = style_for(&important);
        assert_eq!(important_style.display(), DisplayValue::None);
        assert!(important_style.hidden());
        assert_eq!(important_style.opacity(), 64);

        let rollback_style = style_for(&rollback);
        assert_eq!(rollback_style.display(), DisplayValue::Block);
        assert!(!rollback_style.hidden());
        assert_eq!(rollback_style.opacity(), 191);

        let invalid_style = style_for(&invalid);
        assert_eq!(invalid_style.display(), DisplayValue::None);
        assert!(invalid_style.hidden());
        assert_eq!(invalid_style.opacity(), 64);

        let normal_style = style_for(&normal);
        assert_eq!(normal_style.display(), DisplayValue::Block);
        assert!(!normal_style.hidden());
        assert_eq!(normal_style.opacity(), u8::MAX);

        let unlayered_style = style_for(&unlayered);
        assert_eq!(unlayered_style.display(), DisplayValue::Block);
        assert!(!unlayered_style.hidden());
        assert_eq!(unlayered_style.opacity(), 191);

        let inline_rollback_style = style_for(&inline_rollback);
        assert_eq!(inline_rollback_style.display(), DisplayValue::Block);
        assert!(!inline_rollback_style.hidden());
        assert_eq!(inline_rollback_style.opacity(), u8::MAX);
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
        assert_eq!(
            parse_border_declaration("2px solid currentColor"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteCurrentColor {
                    width: 2,
                    style: NativeBorderStyle::Solid,
                }
            ))
        );
        assert_eq!(
            parse_border_declaration("2px HiDdEn red"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteHidden {
                    width: 2,
                    color: NativeColor::RED,
                }
            ))
        );
        assert_eq!(
            parse_border_declaration("2px NoNe red"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteNone {
                    width: 2,
                    color: NativeColor::RED,
                }
            ))
        );
        assert_eq!(
            parse_declarations(
                "border: 2px hidden red; border-top: 3px HIDDEN blue; border-right: 4px hidden green; border-bottom: 5px Hidden black; border-left: 6px hIdDeN white"
            )
            .border,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 3,
                        color: NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 4,
                        color: NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 5,
                        color: NativeColor::BLACK,
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 6,
                        color: NativeColor::WHITE,
                    }
                )),
            ]
        );
        assert_eq!(
            parse_declarations(
                "border: 2px none red; border-top: 3px NoNe blue; border-right: 4px none green; border-bottom: 5px NONE black; border-left: 6px nOnE white"
            )
            .border,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 3,
                        color: NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 4,
                        color: NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 5,
                        color: NativeColor::BLACK,
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 6,
                        color: NativeColor::WHITE,
                    }
                )),
            ]
        );
        assert_eq!(
            parse_border_declaration("ReVeRt"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Revert
            ))
        );
        for (value, declaration) in [
            ("InHeRiT", NativeBorderDeclaration::Inherit),
            ("UNSET", NativeBorderDeclaration::Unset),
            ("initial", NativeBorderDeclaration::Initial),
        ] {
            assert_eq!(
                parse_border_declaration(value),
                Some(LocalCascadeDeclaration::Value(declaration)),
                "value={value}"
            );
        }
        assert_eq!(
            parse_declarations("border: inherit; border: invalid").border,
            [Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Inherit
            )); 4]
        );
        for value in [
            "inherit solid red",
            "2px inherit red",
            "2px solid inherit",
            "inherit 1px",
        ] {
            assert_eq!(parse_border_declaration(value), None, "value={value}");
        }
        assert_eq!(parse_border_declaration("none solid"), None);
        assert_eq!(parse_border_declaration("hidden solid"), None);
        assert_eq!(parse_border_declaration("none red"), None);
        assert_eq!(parse_border_declaration("2px none"), None);
        assert_eq!(parse_border_declaration("2px none red solid"), None);
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
    fn complete_border_current_color_parser_covers_physical_styles_and_rejections() {
        let declarations = parse_declarations(
            "border: 1px solid CURRENTcolor; border-top: 2px DASHED currentColor; border-right: 3px none CuRrEnTcOlOr; border-bottom: 4px HIDDEN CURRENTCOLOR; border-left: 5px double currentColor",
        );
        assert_eq!(
            declarations.border,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteCurrentColor {
                        width: 2,
                        style: NativeBorderStyle::Dashed,
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNoneCurrentColor { width: 3 }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHiddenCurrentColor { width: 4 }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteCurrentColor {
                        width: 5,
                        style: NativeBorderStyle::Double,
                    }
                )),
            ]
        );
        assert_eq!(
            parse_border_declaration("2px solid red"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(border_side(2, NativeColor::RED))
            ))
        );

        for value in [
            "2px solid",
            "2px solid currentColor red",
            "2px wavy currentColor",
            "2px solid revert-layer",
            "2px solid inherit",
            "2px solid hsl(0 100% 50%)",
            "2px none",
            "2px hidden",
        ] {
            assert_eq!(parse_border_declaration(value), None, "{value}");
        }
    }

    #[test]
    fn complete_border_css_wide_values_project_through_component_cascade() {
        let inherited_width = [2, 3, 4, 5];
        let inherited_style = [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
        ];
        let inherited_color = [
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
        ];
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border: inherit; } #physical { border-top: inherit; border-right: unset; border-bottom: initial; border-left: revert; } #reset { border: unset; } #initial { border: initial; } #revert { border: revert; } #omitted {} #invalid { border: inherit; border: invalid; } #mixed { border: inherit solid red; } #compose { color: green; border: unset; border-width: 2px; border-style: solid; } #root { border: inherit; }"
                .into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Border</div>"));
            stylesheet.computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    color: Some(NativeColor::RED),
                    border_color: inherited_color,
                    border_width: inherited_width,
                    border_style: inherited_style,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
        };

        let inherited = computed("inherit").border().unwrap();
        assert_eq!(inherited.top(), border_side(2, NativeColor::RED));
        assert_eq!(inherited.right().width(), 3);
        assert_eq!(inherited.right().style(), NativeBorderStyle::Dashed);
        assert_eq!(inherited.right().color(), inherited_color[1]);
        assert_eq!(inherited.bottom().width(), 4);
        assert_eq!(inherited.bottom().style(), NativeBorderStyle::Dotted);
        assert_eq!(inherited.left().width(), 5);
        assert_eq!(inherited.left().style(), NativeBorderStyle::Double);

        let physical = computed("physical").border().unwrap();
        assert_eq!(physical.top(), border_side(2, NativeColor::RED));
        for side in [physical.right(), physical.bottom(), physical.left()] {
            assert_eq!(side.width(), 0);
            assert_eq!(side.style(), NativeBorderStyle::Solid);
        }
        for id in ["reset", "initial", "revert", "omitted", "mixed"] {
            assert!(computed(id).border().is_none(), "id={id}");
        }
        let invalid = computed("invalid").border().unwrap();
        assert_eq!(invalid.top(), border_side(2, NativeColor::RED));
        let composed = computed("compose").border().unwrap();
        assert_eq!(
            composed.top(),
            border_side(
                2,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                }
            )
        );
        assert_eq!(computed("root").border(), Some(inherited));
    }

    #[test]
    fn logical_border_parsers_expand_pairs_and_preserve_css_wide_rules() {
        assert_eq!(
            parse_logical_border_width_pair("1px 2px"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(1)),
                LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(2)),
            ])
        );
        assert_eq!(
            parse_logical_border_style_pair("NONE hidden"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::None),
                LocalCascadeDeclaration::Value(NativeBorderStyleValue::Hidden),
            ])
        );
        assert_eq!(
            parse_logical_border_color_pair("currentColor #123456"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderColorValue::CurrentColor),
                LocalCascadeDeclaration::Value(NativeBorderColorValue::Color(NativeColor {
                    red: 0x12,
                    green: 0x34,
                    blue: 0x56,
                    alpha: 255,
                })),
            ])
        );
        assert_eq!(
            parse_logical_border_width_pair("ReVeRt-LaYeR"),
            Some([LocalCascadeDeclaration::RevertLayer; 2])
        );
        for value in ["inherit 1px", "1px unset", "1px 2px 3px", "50%"] {
            assert_eq!(
                parse_logical_border_width_pair(value),
                None,
                "value={value}"
            );
        }

        let declarations = parse_declarations(
            "border-inline: 1px solid red; border-block-width: 2px 3px; border-inline-start-style: hidden; border-block-end-color: currentColor",
        );
        assert_eq!(
            declarations.logical_border.border,
            [
                None,
                None,
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::Complete(border_side(1, NativeColor::RED))
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::Complete(border_side(1, NativeColor::RED))
                )),
            ]
        );
        assert_eq!(
            declarations.logical_border.width,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(2)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(3)
                )),
                None,
                None,
            ]
        );
        assert_eq!(
            declarations.logical_border.style,
            [
                None,
                None,
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Hidden
                )),
                None,
            ]
        );
        assert_eq!(
            declarations.logical_border.color,
            [
                None,
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::CurrentColor
                )),
                None,
                None,
            ]
        );
    }

    #[test]
    fn border_color_parser_expands_physical_values_and_rejects_mixed_forms() {
        let declarations = parse_declarations(
            "border-color: red green rgb(1, 2, 3) #123456; border-top-color: transparent; border-right-color: RGBA(4, 5, 6, 0.5); border-bottom-color: ReVeRt-LaYeR; border-left-color: #abcdef",
        );
        assert_eq!(
            declarations.border_color,
            [
                Some(LocalCascadeDeclaration::Value(border_color_value(
                    NativeColor {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 0,
                    }
                ))),
                Some(LocalCascadeDeclaration::Value(border_color_value(
                    NativeColor {
                        red: 4,
                        green: 5,
                        blue: 6,
                        alpha: 128,
                    }
                ))),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(border_color_value(
                    NativeColor {
                        red: 171,
                        green: 205,
                        blue: 239,
                        alpha: 255,
                    }
                ))),
            ]
        );
        assert_eq!(declarations.border_color_order, [1, 2, 3, 4]);
        assert_eq!(
            parse_border_color("red green blue black"),
            Some([
                border_color_value(NativeColor::RED),
                border_color_value(NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                },),
                border_color_value(NativeColor {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },),
                border_color_value(NativeColor::BLACK),
            ])
        );
        assert_eq!(
            parse_border_color("CURRENTcolor red"),
            Some([
                NativeBorderColorValue::CurrentColor,
                border_color_value(NativeColor::RED),
                NativeBorderColorValue::CurrentColor,
                border_color_value(NativeColor::RED),
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
            "rgb(1, 2, 3",
        ] {
            assert_eq!(parse_border_color_declaration(value), None, "{value}");
        }
        assert_eq!(
            parse_border_color_side_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_border_color_side_declaration("CURRENTcolor"),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::CurrentColor
            ))
        );
        for (value, expected) in [
            ("InHeRiT", NativeBorderColorValue::Inherit),
            ("UnSeT", NativeBorderColorValue::Unset),
            ("InItIaL", NativeBorderColorValue::Initial),
            ("ReVeRt", NativeBorderColorValue::Revert),
        ] {
            assert_eq!(
                parse_border_color(value),
                Some([expected; 4]),
                "value={value}"
            );
            assert_eq!(
                parse_border_color_side_declaration(value),
                Some(LocalCascadeDeclaration::Value(expected)),
                "value={value}"
            );
        }
        for value in [
            "inherit red",
            "red inherit",
            "unset blue",
            "initial currentColor",
            "revert green",
            "inherit unset",
        ] {
            assert_eq!(parse_border_color_declaration(value), None, "{value}");
        }
        assert_eq!(parse_border_color_side_declaration("red blue"), None);
    }

    #[test]
    fn border_color_css_wide_keywords_preserve_local_fallback_and_parent_effective_colors() {
        let green = NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        };
        let inherited_border_colors = [NativeColor::RED, green, NativeColor::BLACK, green];
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { color: blue; border: 2px solid red; border-color: inherit; } #unset { color: blue; border: 2px solid red; border-color: unset; } #initial { color: blue; border: 2px solid red; border-color: initial; } #revert { color: blue; border: 2px solid red; border-color: revert; } #current { color: blue; border: 2px solid red; border-color: currentColor; } #omitted { color: blue; border-width: 2px; border-style: solid; } #invalid { color: blue; border: 2px solid red; border-color: inherit; border-color: invalid; }"
                .into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Border</div>"));
            stylesheet.computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    color: Some(NativeColor::RED),
                    border_color: inherited_border_colors,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
        };
        let blue = NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        };
        assert_eq!(computed("inherit").border_colors(), inherited_border_colors);
        assert_eq!(computed("invalid").border_colors(), inherited_border_colors);
        for id in ["unset", "initial", "revert", "current"] {
            assert_eq!(computed(id).border_colors(), [blue; 4], "id={id}");
        }
        assert_eq!(computed("omitted").border_colors(), [NativeColor::BLACK; 4]);
        assert_eq!(
            computed("inherit").border().unwrap().top().color(),
            NativeColor::RED
        );
        assert_eq!(computed("inherit").border().unwrap().right().color(), green);
        assert_eq!(computed("unset").border().unwrap().top().color(), blue);
        assert_eq!(computed("invalid").border().unwrap().left().color(), green);
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
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(5)
                )),
                Some(LocalCascadeDeclaration::RevertLayer),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(6)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(7)
                )),
            ]
        );
        assert_eq!(declarations.border_width_order, [1, 2, 3, 4]);
        assert_eq!(
            parse_border_width_declaration("1px 2px 3px 4px"),
            Some([
                LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(1)),
                LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(2)),
                LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(3)),
                LocalCascadeDeclaration::Value(NativeBorderWidthValue::Width(4)),
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
        for (value, expected) in [
            ("InHeRiT", NativeBorderWidthValue::Inherit),
            ("UnSeT", NativeBorderWidthValue::Unset),
            ("InItIaL", NativeBorderWidthValue::Initial),
            ("ReVeRt", NativeBorderWidthValue::Revert),
        ] {
            assert_eq!(
                parse_border_width_value(value),
                Some(expected),
                "value={value}"
            );
            assert_eq!(
                parse_border_width_declaration(value),
                Some([LocalCascadeDeclaration::Value(expected); 4]),
                "shorthand={value}"
            );
            assert_eq!(
                parse_border_width_side_declaration(value),
                Some(LocalCascadeDeclaration::Value(expected)),
                "longhand={value}"
            );
        }
        for value in ["inherit 1px", "1px unset", "initial 2px 3px", "4px revert"] {
            assert_eq!(parse_border_width_declaration(value), None, "mixed={value}");
        }
        assert_eq!(
            parse_border_width_side_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(parse_border_width_side_declaration("1px 2px"), None);
    }

    #[test]
    fn border_width_css_wide_keywords_resolve_effective_inherited_widths() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border-width: inherit; border-style: solid; border-color: red; } #physical { border-top-width: inherit; border-right-width: unset; border-bottom-width: initial; border-left-width: revert; border-style: solid; border-color: red; } #unset { border-width: unset; border-style: solid; border-color: red; } #initial { border-width: initial; border-style: solid; border-color: red; } #revert { border-width: revert; border-style: solid; border-color: red; } #omitted { border-style: solid; border-color: red; } #invalid { border-width: inherit; border-width: invalid; border-style: solid; border-color: red; }"
                .into(),
        ])
        .unwrap();
        let inherited_width = [2, 3, 4, 5];
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Border</div>"));
            stylesheet.computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    border_width: inherited_width,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
        };

        assert_eq!(computed("inherit").border_widths(), inherited_width);
        assert_eq!(computed("physical").border_widths(), [2, 0, 0, 0]);
        for id in ["unset", "initial", "revert", "omitted"] {
            assert_eq!(computed(id).border_widths(), [0; 4], "id={id}");
        }
        assert_eq!(computed("invalid").border_widths(), inherited_width);

        let inherited_border = computed("inherit").border().unwrap();
        assert_eq!(inherited_border.top().width(), 2);
        assert_eq!(inherited_border.right().width(), 3);
        assert_eq!(inherited_border.bottom().width(), 4);
        assert_eq!(inherited_border.left().width(), 5);
        let physical_border = computed("physical").border().unwrap();
        assert_eq!(physical_border.top().width(), 2);
        assert_eq!(physical_border.right().width(), 0);
        assert_eq!(physical_border.bottom().width(), 0);
        assert_eq!(physical_border.left().width(), 0);
        for id in ["unset", "initial", "revert"] {
            assert_eq!(computed(id).border().unwrap().top().width(), 0, "id={id}");
        }
        assert!(computed("omitted").border().is_none());
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
        for (value, expected) in [
            ("InHeRiT", NativeBorderStyleValue::Inherit),
            ("UnSeT", NativeBorderStyleValue::Unset),
            ("InItIaL", NativeBorderStyleValue::Initial),
            ("ReVeRt", NativeBorderStyleValue::Revert),
        ] {
            assert_eq!(
                parse_border_style_value(value),
                Some(expected),
                "value={value}"
            );
            assert_eq!(
                parse_border_style_declaration(value),
                Some([LocalCascadeDeclaration::Value(expected); 4]),
                "shorthand={value}"
            );
            assert_eq!(
                parse_border_style_side_declaration(value),
                Some(LocalCascadeDeclaration::Value(expected)),
                "longhand={value}"
            );
        }
        for value in [
            "inherit solid",
            "solid unset",
            "initial dashed dotted",
            "double revert",
        ] {
            assert_eq!(parse_border_style_declaration(value), None, "mixed={value}");
        }
    }

    #[test]
    fn border_style_css_wide_keywords_resolve_effective_inherited_styles() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border-style: inherit; border-width: 1px; border-color: red; } #physical { border-top-style: inherit; border-right-style: unset; border-bottom-style: initial; border-left-style: revert; border-width: 1px; border-color: red; } #unset { border-style: unset; border-width: 1px; border-color: red; } #initial { border-style: initial; border-width: 1px; border-color: red; } #revert { border-style: revert; border-width: 1px; border-color: red; } #omitted { border-width: 1px; border-color: red; } #invalid { border-style: inherit; border-style: invalid; border-width: 1px; border-color: red; }"
                .into(),
        ])
        .unwrap();
        let inherited_style = [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
        ];
        let computed = |id: &str| {
            let element = node(&format!("<div id='{id}'>Border</div>"));
            stylesheet.computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    border_style: inherited_style,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
        };

        assert_eq!(computed("inherit").border_styles(), inherited_style);
        assert_eq!(
            computed("physical").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
            ]
        );
        for id in ["unset", "initial", "revert", "omitted"] {
            assert_eq!(
                computed(id).border_styles(),
                [NativeBorderStyleValue::None; 4],
                "id={id}"
            );
        }
        assert_eq!(computed("invalid").border_styles(), inherited_style);

        let inherited_border = computed("inherit").border().unwrap();
        assert_eq!(inherited_border.top().style(), NativeBorderStyle::Solid);
        assert_eq!(inherited_border.top().width(), 1);
        assert_eq!(inherited_border.left().style(), NativeBorderStyle::Dashed);
        assert_eq!(inherited_border.left().width(), 1);
        let physical_border = computed("physical").border().unwrap();
        assert_eq!(physical_border.top().style(), NativeBorderStyle::Solid);
        assert_eq!(physical_border.top().width(), 1);
        assert_eq!(physical_border.right().width(), 0);
        assert!(computed("unset").border().is_none());
        assert!(computed("omitted").border().is_none());
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
    fn background_color_css_wide_keywords_preserve_optional_fallback() {
        let green = NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        };
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #inherit { background-color: inherit; } #unset { background-color: unset; } #initial { background-color: initial; } #revert { background-color: revert; } #invalid { background-color: red; background-color: inherit; background-color: invalid; } #precedence { background-color: red; } } @layer theme { #precedence { background-color: blue; } } #precedence { background-color: inherit; }"
                .into(),
        ])
        .unwrap();
        let computed = |html: &str, inherited_background| {
            let element = node(html);
            stylesheet
                .computed_for_with_matcher(
                    &element,
                    NativeInheritedStyle {
                        background_color: inherited_background,
                        ..NativeInheritedStyle::default()
                    },
                    |selector| selector.matches(&element),
                )
                .background_color()
        };
        assert_eq!(
            parse_background_color_declaration("InHeRiT"),
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::Inherit
            ))
        );
        assert_eq!(
            parse_background_color_declaration("UnSeT"),
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::Unset
            ))
        );
        assert_eq!(
            parse_background_color_declaration("InItIaL"),
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::Initial
            ))
        );
        assert_eq!(
            parse_background_color_declaration("ReVeRt"),
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::Revert
            ))
        );
        assert_eq!(
            parse_background_color_declaration("CuRrEnTcOlOr"),
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::CurrentColor
            ))
        );
        assert_eq!(
            parse_background_color_declaration("ReVeRt-LaYeR"),
            Some(LocalCascadeDeclaration::RevertLayer)
        );
        assert_eq!(
            computed("<div id='inherit'>Inherit</div>", Some(green)),
            Some(green)
        );
        assert_eq!(computed("<div id='unset'>Unset</div>", Some(green)), None);
        assert_eq!(
            computed("<div id='initial'>Initial</div>", Some(green)),
            None
        );
        assert_eq!(computed("<div id='revert'>Revert</div>", Some(green)), None);
        assert_eq!(
            computed("<div id='invalid'>Invalid</div>", Some(green)),
            Some(green)
        );
        assert_eq!(
            computed("<div id='precedence'>Precedence</div>", Some(green)),
            Some(green)
        );
        assert_eq!(computed("<div id='inherit'>Root</div>", None), None);
        assert_eq!(
            NativeStylesheet::default()
                .computed_for(&node("<div id='omitted'>Omitted</div>"))
                .background_color(),
            None
        );
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
    fn overflow_important_parser_tracks_markers_and_invalid_preservation() {
        let declarations = parse_declarations(
            "overflow: hidden !IMPORTANT; overflow-x: clip; overflow-y: visible !important",
        );
        assert_eq!(
            declarations.overflow,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Hidden))
        );
        assert_eq!(
            declarations.overflow_x,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Clip))
        );
        assert_eq!(
            declarations.overflow_y,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Other))
        );
        assert_eq!(
            declarations.overflow_importance,
            NativeOverflowDeclarationImportance {
                shorthand: true,
                x: false,
                y: true,
            }
        );

        let preserved = parse_declarations(
            "overflow: hidden !important; overflow: bad !important; overflow-x: clip !important; overflow-x: bad !important; overflow-y: visible !important; overflow-y: bad !important",
        );
        assert_eq!(
            preserved.overflow,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Hidden))
        );
        assert_eq!(
            preserved.overflow_x,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Clip))
        );
        assert_eq!(
            preserved.overflow_y,
            Some(LocalCascadeDeclaration::Value(OverflowValue::Other))
        );
        assert_eq!(
            preserved.overflow_importance,
            NativeOverflowDeclarationImportance {
                shorthand: true,
                x: true,
                y: true,
            }
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_overflow_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { overflow: hidden !important; }
                #axis { overflow-x: hidden !important; overflow-y: visible !important; }
                #rollback { overflow: hidden !important; overflow: revert-layer !important; }
                #invalid { overflow: hidden !important; overflow: bad !important; overflow-x: clip !important; overflow-x: bad !important; overflow-y: visible !important; overflow-y: bad !important; }
                #normal { overflow: clip; }
            }
            @layer theme {
                #important { overflow: visible !important; }
                #axis { overflow-x: visible !important; overflow-y: hidden !important; }
                #rollback { overflow: clip !important; }
                #normal { overflow: hidden; }
            }
            #important { overflow: clip; }
            #axis { overflow: clip; }
            #rollback { overflow: visible !important; }
            #normal { overflow: visible; }
            #inline { overflow: visible !important; }"#
                .into(),
        ])
        .unwrap();
        let important = node("<div id='important'>Important</div>");
        let axis = node("<div id='axis'>Axis</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let normal = node("<div id='normal'>Normal</div>");
        let inline = node("<div id='inline' style='overflow:hidden !important'>Inline</div>");

        let clips = |element: &NativeNode| {
            let style = stylesheet.computed_for(element);
            (style.overflow_clip_x(), style.overflow_clip_y())
        };
        assert_eq!(clips(&important), (true, true));
        assert_eq!(clips(&axis), (true, false));
        assert_eq!(clips(&rollback), (true, true));
        assert_eq!(clips(&invalid), (true, false));
        assert_eq!(clips(&normal), (false, false));
        assert_eq!(clips(&inline), (true, true));
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
    fn line_height_declaration_parser_accepts_standalone_css_wide_keywords() {
        assert_eq!(
            parse_line_height_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_line_height_declaration("28px"),
            Some(InheritedTextDeclaration::Value(28))
        );
        assert_eq!(
            parse_line_height_declaration(" REVERT-LAYER "),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_line_height_declaration("InHeRiT"),
            Some(InheritedTextDeclaration::Inherit)
        );
        assert_eq!(
            parse_line_height_declaration("INITIAL"),
            Some(InheritedTextDeclaration::Initial)
        );
        assert_eq!(
            parse_line_height_declaration("unset"),
            Some(InheritedTextDeclaration::Unset)
        );
        assert_eq!(
            parse_line_height_declaration("ReVeRt"),
            Some(InheritedTextDeclaration::Revert)
        );
        assert_eq!(parse_line_height_declaration("inherit 28px"), None);
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
    fn inherited_alignment_declaration_parser_accepts_standalone_css_wide_keywords() {
        assert_eq!(
            parse_text_align_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_align_last_declaration(" REVERT-LAYER "),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_justify_declaration("revert-LAYER"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_align_declaration("INHERIT"),
            Some(InheritedTextDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_align_last_declaration("initial"),
            Some(InheritedTextDeclaration::Initial)
        );
        assert_eq!(
            parse_text_justify_declaration("UnSeT"),
            Some(InheritedTextDeclaration::Unset)
        );
        assert_eq!(
            parse_text_align_declaration("ReVeRt"),
            Some(InheritedTextDeclaration::Revert)
        );
        assert_eq!(
            parse_text_align_last_declaration("inherit"),
            Some(InheritedTextDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_justify_declaration("INITIAL"),
            Some(InheritedTextDeclaration::Initial)
        );
        assert_eq!(
            parse_text_align_declaration("unset"),
            Some(InheritedTextDeclaration::Unset)
        );
        assert_eq!(
            parse_text_align_last_declaration("REVERT"),
            Some(InheritedTextDeclaration::Revert)
        );
        assert_eq!(parse_text_align_declaration("center revert-layer"), None);
        assert_eq!(parse_text_justify_declaration("none revert-layer"), None);
    }

    #[test]
    fn white_space_declaration_parser_accepts_bounded_modes_and_css_wide_keywords() {
        assert_eq!(
            parse_white_space_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_white_space_declaration("PRE-WRAP"),
            Some(InheritedTextDeclaration::Value(WhiteSpaceValue::PreWrap))
        );
        assert_eq!(
            parse_white_space_declaration("pre-line"),
            Some(InheritedTextDeclaration::Value(WhiteSpaceValue::PreLine))
        );
        assert_eq!(
            parse_white_space_declaration("InHeRiT"),
            Some(InheritedTextDeclaration::Inherit)
        );
        assert_eq!(
            parse_white_space_declaration("INITIAL"),
            Some(InheritedTextDeclaration::Initial)
        );
        assert_eq!(
            parse_white_space_declaration("unset"),
            Some(InheritedTextDeclaration::Unset)
        );
        assert_eq!(
            parse_white_space_declaration("ReVeRt"),
            Some(InheritedTextDeclaration::Revert)
        );
        assert_eq!(parse_white_space_declaration("inherit pre"), None);
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
    fn direction_declaration_parser_accepts_standalone_css_wide_keywords() {
        assert_eq!(
            parse_direction_declaration("ReVeRt-LaYeR"),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_direction_declaration("RTL"),
            Some(InheritedTextDeclaration::Value(DirectionValue::Rtl))
        );
        assert_eq!(
            parse_direction_declaration(" REVERT-LAYER "),
            Some(InheritedTextDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_direction_declaration("INHERIT"),
            Some(InheritedTextDeclaration::Inherit)
        );
        assert_eq!(
            parse_direction_declaration("initial"),
            Some(InheritedTextDeclaration::Initial)
        );
        assert_eq!(
            parse_direction_declaration("UnSeT"),
            Some(InheritedTextDeclaration::Unset)
        );
        assert_eq!(
            parse_direction_declaration("ReVeRt"),
            Some(InheritedTextDeclaration::Revert)
        );
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
        assert_eq!(
            parse_text_decoration_style("InHeRiT"),
            Some(NativeTextDecorationStyleDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_decoration_style("InItIaL"),
            Some(NativeTextDecorationStyleDeclaration::Initial)
        );
        assert_eq!(
            parse_text_decoration_style("UnSeT"),
            Some(NativeTextDecorationStyleDeclaration::Unset)
        );
        assert_eq!(
            parse_text_decoration_style("ReVeRt"),
            Some(NativeTextDecorationStyleDeclaration::Revert)
        );
        assert_eq!(parse_text_decoration_style("zigzag"), None);
        assert_eq!(parse_text_decoration_style("solid double"), None);
        assert_eq!(parse_text_decoration_style("inherit initial"), None);
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
        assert_eq!(
            parse_text_decoration_skip_ink("InHeRiT"),
            Some(NativeTextDecorationSkipInkDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_decoration_skip_ink("InItIaL"),
            Some(NativeTextDecorationSkipInkDeclaration::Initial)
        );
        assert_eq!(
            parse_text_decoration_skip_ink("UnSeT"),
            Some(NativeTextDecorationSkipInkDeclaration::Unset)
        );
        assert_eq!(
            parse_text_decoration_skip_ink("ReVeRt"),
            Some(NativeTextDecorationSkipInkDeclaration::Revert)
        );
        assert_eq!(parse_text_decoration_skip_ink("all"), None);
        assert_eq!(parse_text_decoration_skip_ink("inherit initial"), None);
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
    fn text_decoration_skip_ink_css_wide_resets_follow_parent_and_initial() {
        let document = NativeDocument::parse(
            "<style>#parent { text-decoration-skip-ink: none; } #initial { text-decoration-skip-ink: initial; } #inherit { text-decoration-skip-ink: inherit; } #unset { text-decoration-skip-ink: unset; } #revert { text-decoration-skip-ink: revert; } #invalid { text-decoration-skip-ink: none; text-decoration-skip-ink: all; } #important { text-decoration-skip-ink: none !important; } #important { text-decoration-skip-ink: initial; }</style><div id='parent'><span id='initial'>Initial</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='invalid'>Invalid</span><span id='important'>Important</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let style = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
                .text_decoration_skip_ink()
        };

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_decoration_skip_ink(),
            NativeTextDecorationSkipInk::Auto
        );
        assert_eq!(style("parent"), NativeTextDecorationSkipInk::None);
        assert_eq!(style("initial"), NativeTextDecorationSkipInk::Auto);
        assert_eq!(style("inherit"), NativeTextDecorationSkipInk::None);
        assert_eq!(style("unset"), NativeTextDecorationSkipInk::None);
        assert_eq!(style("revert"), NativeTextDecorationSkipInk::None);
        assert_eq!(style("invalid"), NativeTextDecorationSkipInk::None);
        assert_eq!(style("important"), NativeTextDecorationSkipInk::None);
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
        assert_eq!(
            parse_text_decoration_thickness("InHeRiT"),
            Some(NativeTextDecorationThicknessDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_decoration_thickness("InItIaL"),
            Some(NativeTextDecorationThicknessDeclaration::Initial)
        );
        assert_eq!(
            parse_text_decoration_thickness("UnSeT"),
            Some(NativeTextDecorationThicknessDeclaration::Unset)
        );
        assert_eq!(
            parse_text_decoration_thickness("ReVeRt"),
            Some(NativeTextDecorationThicknessDeclaration::Revert)
        );
        assert_eq!(parse_text_decoration_thickness("0px"), None);
        assert_eq!(parse_text_decoration_thickness("5px"), None);
        assert_eq!(parse_text_decoration_thickness("auto"), None);
        assert_eq!(parse_text_decoration_thickness("1.5px"), None);
        assert_eq!(parse_text_decoration_thickness("2px 3px"), None);
        assert_eq!(parse_text_decoration_thickness("from-font"), None);
        assert_eq!(parse_text_decoration_thickness("inherit initial"), None);
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
    fn text_decoration_thickness_css_wide_resets_follow_parent_and_initial() {
        let document = NativeDocument::parse(
            "<style>#parent { text-decoration-thickness: 3px; } #initial { text-decoration-thickness: initial; } #inherit { text-decoration-thickness: inherit; } #unset { text-decoration-thickness: unset; } #revert { text-decoration-thickness: revert; } #invalid { text-decoration-thickness: 4px; text-decoration-thickness: 5px; } #important { text-decoration-thickness: 3px !important; } #important { text-decoration-thickness: 1px; }</style><div id='parent'><span id='initial'>Initial</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='invalid'>Invalid</span><span id='important'>Important</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let style = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
                .text_decoration_thickness()
        };

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_decoration_thickness(),
            1
        );
        assert_eq!(style("parent"), 3);
        assert_eq!(style("initial"), 1);
        assert_eq!(style("inherit"), 3);
        assert_eq!(style("unset"), 3);
        assert_eq!(style("revert"), 3);
        assert_eq!(style("invalid"), 4);
        assert_eq!(style("important"), 3);
    }

    #[test]
    fn text_underline_offset_parser_accepts_css_wide_resets_and_bounded_signed_pixels() {
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
        assert_eq!(
            parse_text_underline_offset("InHeRiT"),
            Some(NativeTextUnderlineOffsetDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_underline_offset("InItIaL"),
            Some(NativeTextUnderlineOffsetDeclaration::Initial)
        );
        assert_eq!(
            parse_text_underline_offset("UnSeT"),
            Some(NativeTextUnderlineOffsetDeclaration::Unset)
        );
        assert_eq!(
            parse_text_underline_offset("ReVeRt"),
            Some(NativeTextUnderlineOffsetDeclaration::Revert)
        );
        assert_eq!(parse_text_underline_offset("-5px"), None);
        assert_eq!(parse_text_underline_offset("5px"), None);
        assert_eq!(parse_text_underline_offset("+1px"), None);
        assert_eq!(parse_text_underline_offset("1.5px"), None);
        assert_eq!(parse_text_underline_offset("auto"), None);
        assert_eq!(parse_text_underline_offset("10%"), None);
        assert_eq!(parse_text_underline_offset("2px 3px"), None);
        assert_eq!(parse_text_underline_offset("inherit initial"), None);
    }

    #[test]
    fn text_underline_offset_css_wide_resets_follow_parent_and_initial() {
        let document = NativeDocument::parse(
            "<style>#parent { text-underline-offset: -3px; } #initial { text-underline-offset: initial; } #inherit { text-underline-offset: inherit; } #unset { text-underline-offset: unset; } #revert { text-underline-offset: revert; } #invalid { text-underline-offset: 4px; text-underline-offset: 5px; } #important { text-underline-offset: -2px !important; } #important { text-underline-offset: 0px; }</style><div id='parent'><span id='initial'>Initial</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='invalid'>Invalid</span><span id='important'>Important</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let style = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
                .text_underline_offset()
        };

        assert_eq!(
            document
                .computed_style_for_layout(document.root())
                .text_underline_offset(),
            0
        );
        assert_eq!(style("parent"), -3);
        assert_eq!(style("initial"), 0);
        assert_eq!(style("inherit"), -3);
        assert_eq!(style("unset"), -3);
        assert_eq!(style("revert"), -3);
        assert_eq!(style("invalid"), 4);
        assert_eq!(style("important"), -2);
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
            Some(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: u8::MAX,
            })
        );
    }

    #[test]
    fn text_decoration_color_parser_accepts_bounded_values_and_css_wide_keywords() {
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
        assert_eq!(
            parse_text_decoration_color("CuRrEnTcOlOr"),
            Some(NativeTextDecorationColorDeclaration::CurrentColor)
        );
        for (value, expected) in [
            ("InHeRiT", NativeTextDecorationColorDeclaration::Inherit),
            ("UnSeT", NativeTextDecorationColorDeclaration::Unset),
            ("InItIaL", NativeTextDecorationColorDeclaration::Initial),
            ("ReVeRt", NativeTextDecorationColorDeclaration::Revert),
        ] {
            assert_eq!(
                parse_text_decoration_color(value),
                Some(expected),
                "value={value}"
            );
        }
        assert_eq!(
            parse_text_decoration_color("linear-gradient(red, blue)"),
            None
        );
        assert_eq!(
            parse_declarations("text-decoration-color: revert-layer").text_decoration_color,
            Some(NativeTextDecorationColorDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_declarations(
                "text-decoration-color: red; text-decoration-color: currentColor; text-decoration-color: invalid"
            )
            .text_decoration_color,
            Some(NativeTextDecorationColorDeclaration::CurrentColor)
        );
    }

    #[test]
    fn text_decoration_color_css_wide_keywords_preserve_local_fallback_and_parent_effective_color()
    {
        let green = NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        };
        let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { text-decoration-color: inherit; } #unset { text-decoration-color: unset; } #initial { text-decoration-color: initial; } #revert { text-decoration-color: revert; } #current { text-decoration-color: currentColor; } #invalid { text-decoration-color: red; text-decoration-color: inherit; text-decoration-color: invalid; }".into(),
        ])
        .unwrap();
        let computed = |id: &str, inherited_decoration_color| {
            let element = node(&format!("<div id='{id}'>Text</div>"));
            stylesheet
                .computed_for_with_matcher(
                    &element,
                    NativeInheritedStyle {
                        color: Some(green),
                        text_decoration_color: inherited_decoration_color,
                        ..NativeInheritedStyle::default()
                    },
                    |selector| selector.matches(&element),
                )
                .text_decoration_color()
        };
        assert_eq!(
            computed("inherit", NativeColor::RED),
            Some(NativeColor::RED)
        );
        for id in ["unset", "initial", "revert", "current"] {
            assert_eq!(computed(id, NativeColor::RED), Some(green), "id={id}");
        }
        assert_eq!(
            computed("invalid", NativeColor::RED),
            Some(NativeColor::RED)
        );
        let omitted = stylesheet
            .computed_for_with_matcher(
                &node("<div>Text</div>"),
                NativeInheritedStyle {
                    color: Some(green),
                    text_decoration_color: NativeColor::RED,
                    ..NativeInheritedStyle::default()
                },
                |_| false,
            )
            .text_decoration_color();
        assert_eq!(omitted, None);
    }

    #[test]
    fn paint_color_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "background-color: red !IMPORTANT; color: blue !important; text-decoration-color: green !important; background-color: invalid !important;",
        );
        assert_eq!(
            declarations.background_color,
            Some(LocalCascadeDeclaration::Value(
                NativeBackgroundColorValue::Color(NativeColor::RED),
            ))
        );
        assert!(declarations.background_color_important);
        assert_eq!(
            declarations.color,
            Some(LocalCascadeDeclaration::Value(NativeColorValue::Color(
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: u8::MAX,
                    alpha: u8::MAX,
                },
            )))
        );
        assert!(declarations.color_important);
        assert_eq!(
            declarations.text_decoration_color,
            Some(NativeTextDecorationColorDeclaration::Value(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: u8::MAX,
            }))
        );
        assert!(declarations.text_decoration_color_important);
    }

    #[test]
    fn stylesheet_cascade_resolves_paint_color_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { background-color: red !important; color: red !important; text-decoration-color: red !important; } #rollback { background-color: red !important; background-color: revert-layer !important; color: red !important; color: revert-layer !important; text-decoration-color: red !important; text-decoration-color: revert-layer !important; } #normal { background-color: red; color: red; text-decoration-color: red; } #mixed { background-color: red !important; color: red !important; text-decoration-color: red !important; } } @layer theme { #layered { background-color: blue !important; color: blue !important; text-decoration-color: blue !important; } #rollback { background-color: green !important; color: green !important; text-decoration-color: green !important; } #normal { background-color: blue; color: blue; text-decoration-color: blue; } } #layered { background-color: green !important; color: green !important; text-decoration-color: green !important; } #normal { background-color: green; color: green; text-decoration-color: green; } #mixed { background-color: green; color: green; text-decoration-color: green; } #inline { background-color: red !important; color: red !important; text-decoration-color: red !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            let style = stylesheet.computed_for(&node(&format!("<div id='{id}'>Text</div>")));
            (
                style.background_color(),
                style.color(),
                style.text_decoration_color(),
            )
        };
        let red = NativeColor::RED;
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

        assert_eq!(computed("layered"), (Some(red), Some(red), Some(red)));
        assert_eq!(
            computed("rollback"),
            (Some(green), Some(green), Some(green))
        );
        assert_eq!(computed("normal"), (Some(green), Some(green), Some(green)));
        assert_eq!(computed("mixed"), (Some(red), Some(red), Some(red)));

        let inline = node(
            "<div id='inline' style='background-color:green !important; color:green !important; text-decoration-color:green !important'>Inline</div>",
        );
        assert_eq!(
            stylesheet.computed_for(&inline).background_color(),
            Some(green)
        );
        assert_eq!(stylesheet.computed_for(&inline).color(), Some(green));
        assert_eq!(
            stylesheet.computed_for(&inline).text_decoration_color(),
            Some(green)
        );
        assert_ne!(computed("layered").0, Some(blue));
    }

    #[test]
    fn physical_border_color_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-color: red !IMPORTANT; border-top-color: blue !important; border-right-color: invalid !important; border-right-color: green;",
        );
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
        assert_eq!(
            declarations.border_color,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(blue)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(green)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(NativeColor::RED)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(NativeColor::RED)
                )),
            ]
        );
        assert_eq!(
            declarations.border_color_important,
            [true, false, true, true]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_physical_border_color_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { border-color: red !important; } #rollback { border-color: red !important; } #rollback { border-color: revert-layer !important; } #normal { border-color: red; } #mixed { border-color: red !important; } #sides { border-color: red !important; border-top-color: blue !important; border-right-color: green !important; } #sides { border-bottom-color: blue; border-left-color: green; } #invalid { border-color: red !important; border-color: invalid !important; } } @layer theme { #layered { border-color: blue !important; } #rollback { border-color: green !important; } #normal { border-color: blue; } #sides { border-color: green !important; } } #layered { border-color: green !important; } #normal { border-color: green; } #mixed { border-color: green; } #sides { border-color: green !important; } #inline { border-color: red !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
                .border_colors()
        };
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

        assert_eq!(computed("layered"), [NativeColor::RED; 4]);
        assert_eq!(computed("rollback"), [green; 4]);
        assert_eq!(computed("normal"), [green; 4]);
        assert_eq!(computed("mixed"), [NativeColor::RED; 4]);
        assert_eq!(
            computed("sides"),
            [blue, green, NativeColor::RED, NativeColor::RED]
        );
        assert_eq!(computed("invalid"), [NativeColor::RED; 4]);

        let inline = node("<div id='inline' style='border-color:green !important'>Inline</div>");
        assert_eq!(stylesheet.computed_for(&inline).border_colors(), [green; 4]);
    }

    #[test]
    fn physical_border_width_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-width: 1px !IMPORTANT; border-top-width: 2px !important; border-right-width: invalid !important; border-right-width: 3px;",
        );
        assert_eq!(
            declarations.border_width,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(2)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(3)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(1)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(1)
                )),
            ]
        );
        assert_eq!(
            declarations.border_width_important,
            [true, false, true, true]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_physical_border_width_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #early { border-width: 1px !important; } #rollback { border-width: 1px !important; } #rollback { border-width: revert-layer !important; } #normal { border-width: 1px; } #mixed { border-top-width: 1px !important; } #sides { border-width: 1px !important; border-top-width: 2px !important; border-right-width: 3px !important; } #invalid { border-width: 1px !important; border-width: invalid !important; } } @layer theme { #early { border-width: 2px !important; } #rollback { border-width: 3px !important; } #normal { border-width: 2px; } #sides { border-width: 4px !important; } } #early { border-width: 3px !important; } #normal { border-width: 3px; } #mixed { border-width: 4px; } #sides { border-width: 5px !important; } #invalid { border-width: 5px; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
                .border_widths()
        };

        assert_eq!(computed("early"), [1; 4]);
        assert_eq!(computed("rollback"), [3; 4]);
        assert_eq!(computed("normal"), [3; 4]);
        assert_eq!(computed("mixed"), [1, 4, 4, 4]);
        assert_eq!(computed("sides"), [2, 3, 1, 1]);
        assert_eq!(computed("invalid"), [1; 4]);

        let inline = node("<div id='inline' style='border-width:4px !important'>Inline</div>");
        assert_eq!(stylesheet.computed_for(&inline).border_widths(), [4; 4]);
    }

    #[test]
    fn physical_border_style_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-style: solid !IMPORTANT; border-top-style: dashed !important; border-right-style: invalid !important; border-right-style: dotted;",
        );
        assert_eq!(
            declarations.border_style,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
                )),
            ]
        );
        assert_eq!(
            declarations.border_style_important,
            [true, false, true, true]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_physical_border_style_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #early { border-style: solid !important; } #rollback { border-style: dashed !important; } #rollback { border-style: revert-layer !important; } #normal { border-style: dotted; } #mixed { border-top-style: hidden !important; } #sides { border-style: solid !important; border-top-style: none !important; border-right-style: hidden !important; } #invalid { border-style: double !important; border-style: invalid !important; } } @layer theme { #early { border-style: dashed !important; } #rollback { border-style: groove !important; } #normal { border-style: double; } #sides { border-style: ridge !important; } } #early { border-style: dotted !important; } #normal { border-style: outset; } #mixed { border-style: double; } #sides { border-style: inset !important; } #invalid { border-style: dashed; } #wide { border-style: inherit !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
                .border_styles()
        };

        assert_eq!(
            computed("early"),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
        );
        assert_eq!(
            computed("rollback"),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Groove); 4]
        );
        assert_eq!(
            computed("normal"),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Outset); 4]
        );
        assert_eq!(
            computed("mixed"),
            [
                NativeBorderStyleValue::Hidden,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            ]
        );
        assert_eq!(
            computed("sides"),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Hidden,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("invalid"),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Double); 4]
        );

        let inline = node("<div id='inline' style='border-style:hidden !important'>Inline</div>");
        assert_eq!(
            stylesheet.computed_for(&inline).border_styles(),
            [NativeBorderStyleValue::Hidden; 4]
        );
        let inherited_style = [NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted); 4];
        let wide = node("<div id='wide'>Wide</div>");
        assert_eq!(
            stylesheet
                .computed_for_with_matcher(
                    &wide,
                    NativeInheritedStyle {
                        border_style: inherited_style,
                        ..NativeInheritedStyle::default()
                    },
                    |selector| selector.matches(&wide),
                )
                .border_styles(),
            inherited_style
        );
    }

    #[test]
    fn physical_border_shorthand_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border: 1px solid red !IMPORTANT; border-top: 2px dashed blue !important; border-right: invalid !important; border-right: 3px dotted green;",
        );
        assert_eq!(declarations.border_important, [true, false, true, true]);
    }

    #[test]
    fn stylesheet_cascade_resolves_physical_border_shorthand_important_priority_and_composition() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #early { border:1px solid red !important; } #rollback { border:1px solid blue !important; } #rollback { border:revert-layer !important; } #normal { border:1px solid red; } #mixed { border-top:1px solid blue !important; } #sides { border:1px solid red !important; border-top:2px dashed green !important; border-right:3px dotted blue !important; } #invalid { border:1px solid red !important; border:invalid !important; } #none { border:2px solid red !important; border-top:none !important; } #hidden { border:2px solid red !important; border-right:hidden !important; } #wide { border:inherit !important; } } @layer theme { #early { border:2px dashed blue !important; } #rollback { border:3px groove green !important; } #normal { border:2px dashed blue; } #sides { border:4px groove green !important; } } #early { border:3px dotted green !important; } #normal { border:3px double green; } #mixed { border:4px double black; } #sides { border:5px ridge red !important; } #invalid { border:5px outset green; } #none { border-top:4px solid blue; } #hidden { border-right:4px solid blue; }".into(),
        ])
        .unwrap();
        let computed =
            |id: &str| stylesheet.computed_for(&node(&format!("<div id='{id}'>Text</div>")));
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

        assert_eq!(computed("early").border_widths(), [1; 4]);
        assert_eq!(
            computed("early").border_styles(),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
        );
        assert_eq!(computed("early").border_colors(), [NativeColor::RED; 4]);

        assert_eq!(computed("rollback").border_widths(), [3; 4]);
        assert_eq!(
            computed("rollback").border_styles(),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Groove); 4]
        );
        assert_eq!(computed("rollback").border_colors(), [green; 4]);

        assert_eq!(computed("normal").border_widths(), [3; 4]);
        assert_eq!(
            computed("normal").border_styles(),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Double); 4]
        );
        assert_eq!(computed("normal").border_colors(), [green; 4]);

        assert_eq!(computed("mixed").border_widths(), [1, 4, 4, 4]);
        assert_eq!(
            computed("mixed").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            ]
        );
        assert_eq!(
            computed("mixed").border_colors(),
            [
                blue,
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK
            ]
        );

        assert_eq!(computed("sides").border_widths(), [2, 3, 1, 1]);
        assert_eq!(
            computed("sides").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("sides").border_colors(),
            [green, blue, NativeColor::RED, NativeColor::RED]
        );

        assert_eq!(computed("invalid").border_widths(), [1; 4]);
        assert_eq!(
            computed("invalid").border_styles(),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
        );
        assert_eq!(computed("invalid").border_colors(), [NativeColor::RED; 4]);

        assert_eq!(computed("none").border_widths(), [4, 2, 2, 2]);
        assert_eq!(
            computed("none").border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("none").border_colors(),
            [blue, NativeColor::RED, NativeColor::RED, NativeColor::RED]
        );
        assert_eq!(computed("hidden").border_widths(), [2, 4, 2, 2]);
        assert_eq!(
            computed("hidden").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Hidden,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("hidden").border_colors(),
            [NativeColor::RED, blue, NativeColor::RED, NativeColor::RED]
        );

        let inherited = node("<div id='wide'>Wide</div>");
        let inherited_width = [9, 8, 7, 6];
        let inherited_style = [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
        ];
        let inherited_color = [NativeColor::RED, blue, green, NativeColor::BLACK];
        let wide = stylesheet.computed_for_with_matcher(
            &inherited,
            NativeInheritedStyle {
                border_width: inherited_width,
                border_style: inherited_style,
                border_color: inherited_color,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&inherited),
        );
        assert_eq!(wide.border_widths(), inherited_width);
        assert_eq!(wide.border_styles(), inherited_style);
        assert_eq!(wide.border_colors(), inherited_color);

        let inline =
            node("<div id='inline' style='border:6px groove blue !important'>Inline</div>");
        let inline_style = stylesheet.computed_for(&inline);
        assert_eq!(inline_style.border_widths(), [6; 4]);
        assert_eq!(
            inline_style.border_styles(),
            [NativeBorderStyleValue::Paint(NativeBorderStyle::Groove); 4]
        );
        assert_eq!(inline_style.border_colors(), [blue; 4]);
    }

    #[test]
    fn logical_border_color_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-block-color: red !IMPORTANT; border-inline-start-color: blue !important; border-inline-end-color: invalid !important; border-inline-end-color: green;",
        );
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
        assert_eq!(
            declarations.logical_border.color,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(NativeColor::RED)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(NativeColor::RED)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(blue)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderColorValue::Color(green)
                )),
            ]
        );
        assert_eq!(
            declarations.logical_border.color_important,
            [true, true, true, false]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_logical_border_color_important_priority_and_projection() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #start { direction:ltr; border-inline-start-color:red !important; } #rtl { direction:rtl; border-inline-start-color:red !important; } #rollback { border-block-color:red !important; } #rollback { border-block-color:revert-layer !important; } #normal { border-block-color:red; } #mixed { border-inline-color:red !important; } #pair { border-block-color:red blue !important; } } @layer theme { #start { border-inline-start-color:blue !important; } #rtl { border-inline-start-color:blue !important; } #rollback { border-block-color:green !important; } #normal { border-block-color:blue; } #pair { border-block-color:green green !important; } } #start { border-inline-start-color:green !important; } #rtl { border-inline-start-color:green !important; } #normal { border-block-color:green; } #mixed { border-inline-color:green; } #pair { border-block-color:green green !important; } #inline { border-inline-start-color:red !important; } #rollback { border-block-color:green !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
                .border_colors()
        };
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

        assert_eq!(
            computed("start"),
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::RED
            ]
        );
        assert_eq!(
            computed("rtl"),
            [
                NativeColor::BLACK,
                NativeColor::RED,
                NativeColor::BLACK,
                NativeColor::BLACK
            ]
        );
        assert_eq!(
            computed("rollback"),
            [green, NativeColor::BLACK, green, NativeColor::BLACK]
        );
        assert_eq!(
            computed("normal"),
            [green, NativeColor::BLACK, green, NativeColor::BLACK]
        );
        assert_eq!(
            computed("mixed"),
            [
                NativeColor::BLACK,
                NativeColor::RED,
                NativeColor::BLACK,
                NativeColor::RED
            ]
        );
        assert_eq!(
            computed("pair"),
            [
                NativeColor::RED,
                NativeColor::BLACK,
                blue,
                NativeColor::BLACK
            ]
        );

        let inline = node(
            "<div id='inline' style='border-inline-start-color:green !important'>Inline</div>",
        );
        assert_eq!(
            stylesheet.computed_for(&inline).border_colors(),
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                green
            ]
        );
    }

    #[test]
    fn logical_border_width_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-block-width: 1px 2px !IMPORTANT; border-inline-start-width: 3px !important; border-inline-end-width: invalid !important; border-inline-end-width: 4px;",
        );
        assert_eq!(
            declarations.logical_border.width,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(1)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(2)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(3)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderWidthValue::Width(4)
                )),
            ]
        );
        assert_eq!(
            declarations.logical_border.width_important,
            [true, true, true, false]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_logical_border_width_important_priority_and_projection() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; border-inline-start-width:1px !important; } #rtl { direction:rtl; border-inline-start-width:1px !important; } #pair { border-block-width:1px 2px !important; } #rollback { border-block-width:1px !important; } #rollback { border-block-width:revert-layer !important; } #normal { border-block-width:1px; } #mixed { border-inline-width:1px !important; } #invalid { border-inline-width:1px !important; border-inline-width:invalid !important; } } @layer theme { #ltr { border-inline-start-width:2px !important; } #rtl { border-inline-start-width:2px !important; } #pair { border-block-width:3px 4px !important; } #rollback { border-block-width:5px !important; } #normal { border-block-width:2px; } } #ltr { border-inline-start-width:3px !important; } #rtl { border-inline-start-width:3px !important; } #normal { border-block-width:3px; } #mixed { border-inline-width:4px; } #invalid { border-inline-width:4px; } #wide { border-block-width:inherit !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
                .border_widths()
        };

        assert_eq!(computed("ltr"), [0, 0, 0, 1]);
        assert_eq!(computed("rtl"), [0, 1, 0, 0]);
        assert_eq!(computed("pair"), [1, 0, 2, 0]);
        assert_eq!(computed("rollback"), [5, 0, 5, 0]);
        assert_eq!(computed("normal"), [3, 0, 3, 0]);
        assert_eq!(computed("mixed"), [0, 1, 0, 1]);
        assert_eq!(computed("invalid"), [0, 1, 0, 1]);

        let inline =
            node("<div id='inline' style='border-inline-start-width:4px !important'>Inline</div>");
        assert_eq!(
            stylesheet.computed_for(&inline).border_widths(),
            [0, 0, 0, 4]
        );

        let inherited_width = [2, 3, 4, 5];
        let wide = node("<div id='wide'>Wide</div>");
        let wide_style = stylesheet.computed_for_with_matcher(
            &wide,
            NativeInheritedStyle {
                border_width: inherited_width,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&wide),
        );
        assert_eq!(wide_style.border_widths(), [2, 0, 4, 0]);
    }

    #[test]
    fn logical_border_style_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-block-style: solid !IMPORTANT; border-inline-start-style: hidden !important; border-inline-end-style: invalid !important; border-inline-end-style: dotted;",
        );
        assert_eq!(
            declarations.logical_border.style,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Solid,)
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Hidden
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted,)
                )),
            ]
        );
        assert_eq!(
            declarations.logical_border.style_important,
            [true, true, true, false]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_logical_border_style_important_priority_and_projection() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; border-inline-start-style:dashed !important; } #rtl { direction:rtl; border-inline-start-style:dashed !important; } #pair { border-block-style:double dotted !important; } #rollback { border-block-style:groove !important; } #rollback { border-block-style:revert-layer !important; } #normal { border-block-style:dotted; } #mixed { border-inline-style:hidden !important; } #invalid { border-inline-style:double !important; border-inline-style:invalid !important; } #none { border-block-style:none !important; } } @layer theme { #ltr { border-inline-start-style:groove !important; } #rtl { border-inline-start-style:groove !important; } #pair { border-block-style:ridge outset !important; } #rollback { border-block-style:inset !important; } #normal { border-block-style:double; } } #ltr { border-inline-start-style:solid !important; } #rtl { border-inline-start-style:solid !important; } #normal { border-block-style:outset; } #mixed { border-inline-style:double; } #invalid { border-inline-style:solid; } #none { border-block-style:solid; } #wide { border-block-style:inherit !important; }".into(),
        ])
        .unwrap();
        let computed = |id: &str| {
            stylesheet
                .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
                .border_styles()
        };

        assert_eq!(
            computed("ltr"),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            ]
        );
        assert_eq!(
            computed("rtl"),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("pair"),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("rollback"),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("normal"),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Outset),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Outset),
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("mixed"),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Hidden,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Hidden,
            ]
        );
        assert_eq!(
            computed("invalid"),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            ]
        );
        assert_eq!(
            computed("none"),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
            ]
        );

        let inline = node(
            "<div id='inline' style='border-inline-start-style:hidden !important'>Inline</div>",
        );
        assert_eq!(
            stylesheet.computed_for(&inline).border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Hidden,
            ]
        );
        let inherited_style = [NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted); 4];
        let wide = node("<div id='wide'>Wide</div>");
        assert_eq!(
            stylesheet
                .computed_for_with_matcher(
                    &wide,
                    NativeInheritedStyle {
                        border_style: inherited_style,
                        ..NativeInheritedStyle::default()
                    },
                    |selector| selector.matches(&wide),
                )
                .border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
                NativeBorderStyleValue::None,
            ]
        );
    }

    #[test]
    fn logical_border_shorthand_important_parser_tracks_valid_terminal_markers() {
        let declarations = parse_declarations(
            "border-block: 1px solid red !IMPORTANT; border-inline-start: 2px dashed blue !important; border-inline-end: invalid !important; border-inline-end: 3px dotted green;",
        );
        assert_eq!(
            declarations.logical_border.border_important,
            [true, true, true, false]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_logical_border_shorthand_important_priority_and_projection() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; border-inline-start:1px solid red !important; } #rtl { direction:rtl; border-inline-start:1px solid red !important; } #pair { border-block:1px solid red !important; border-inline:2px dashed blue !important; } #rollback { border-block:1px solid red !important; } #rollback { border-block:revert-layer !important; } #normal { border-block:1px solid red; } #mixed { border-inline-start:1px solid blue !important; } #sides { border-block:1px solid red !important; border-block-start:2px dashed green !important; border-inline-end:3px dotted blue !important; } #invalid { border-block:1px solid red !important; border-block:invalid !important; } #none { border-block:2px solid red !important; border-block-start:none !important; } #hidden { border-inline:2px solid red !important; border-inline-end:hidden !important; } #wide { border-block:inherit !important; } } @layer theme { #ltr { border-inline-start:2px dashed blue !important; } #rtl { border-inline-start:2px dashed blue !important; } #rollback { border-block:3px groove green !important; } #normal { border-block:2px dashed blue; } #sides { border-block:4px groove green !important; } } #ltr { border-inline-start:3px dotted green !important; } #rtl { border-inline-start:3px dotted green !important; } #normal { border-block:3px double green; } #mixed { border-inline:4px double black; } #sides { border-inline:5px ridge red !important; } #none { border-block-start:4px solid blue; } #hidden { border-inline-end:4px solid blue; }".into(),
        ])
        .unwrap();
        let computed =
            |id: &str| stylesheet.computed_for(&node(&format!("<div id='{id}'>Text</div>")));
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

        assert_eq!(computed("ltr").border_widths(), [0, 0, 0, 1]);
        assert_eq!(
            computed("ltr").border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("ltr").border_colors(),
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::RED
            ]
        );
        assert_eq!(computed("rtl").border_widths(), [0, 1, 0, 0]);

        assert_eq!(computed("pair").border_widths(), [1, 2, 1, 2]);
        assert_eq!(
            computed("pair").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            ]
        );
        assert_eq!(
            computed("pair").border_colors(),
            [NativeColor::RED, blue, NativeColor::RED, blue]
        );

        assert_eq!(computed("rollback").border_widths(), [3, 0, 3, 0]);
        assert_eq!(
            computed("rollback").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("rollback").border_colors(),
            [green, NativeColor::BLACK, green, NativeColor::BLACK]
        );

        assert_eq!(computed("normal").border_widths(), [3, 0, 3, 0]);
        assert_eq!(
            computed("normal").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("normal").border_colors(),
            [green, NativeColor::BLACK, green, NativeColor::BLACK]
        );

        assert_eq!(computed("mixed").border_widths(), [0, 4, 0, 1]);
        assert_eq!(
            computed("mixed").border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("mixed").border_colors(),
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                blue
            ]
        );

        assert_eq!(computed("sides").border_widths(), [2, 3, 1, 5]);
        assert_eq!(
            computed("sides").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge),
            ]
        );
        assert_eq!(
            computed("sides").border_colors(),
            [green, blue, NativeColor::RED, NativeColor::RED]
        );

        assert_eq!(computed("invalid").border_widths(), [1, 0, 1, 0]);
        assert_eq!(
            computed("invalid").border_styles(),
            [
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::None,
            ]
        );

        assert_eq!(computed("none").border_widths(), [4, 0, 2, 0]);
        assert_eq!(
            computed("none").border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
                NativeBorderStyleValue::None,
            ]
        );
        assert_eq!(
            computed("none").border_colors(),
            [
                blue,
                NativeColor::BLACK,
                NativeColor::RED,
                NativeColor::BLACK
            ]
        );

        assert_eq!(computed("hidden").border_widths(), [0, 4, 0, 2]);
        assert_eq!(
            computed("hidden").border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Hidden,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            ]
        );
        assert_eq!(
            computed("hidden").border_colors(),
            [
                NativeColor::BLACK,
                blue,
                NativeColor::BLACK,
                NativeColor::RED
            ]
        );

        let inherited = node("<div id='wide'>Wide</div>");
        let inherited_width = [9, 8, 7, 6];
        let inherited_style = [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
        ];
        let inherited_color = [NativeColor::RED, blue, green, NativeColor::BLACK];
        let wide = stylesheet.computed_for_with_matcher(
            &inherited,
            NativeInheritedStyle {
                border_width: inherited_width,
                border_style: inherited_style,
                border_color: inherited_color,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&inherited),
        );
        assert_eq!(wide.border_widths(), [9, 0, 7, 0]);
        assert_eq!(
            wide.border_styles(),
            [
                inherited_style[0],
                NativeBorderStyleValue::None,
                inherited_style[2],
                NativeBorderStyleValue::None
            ]
        );
        assert_eq!(
            wide.border_colors(),
            [
                inherited_color[0],
                NativeColor::BLACK,
                inherited_color[2],
                NativeColor::BLACK
            ]
        );

        let inline = node(
            "<div id='inline' style='border-inline-start:6px groove blue !important'>Inline</div>",
        );
        let inline_style = stylesheet.computed_for(&inline);
        assert_eq!(inline_style.border_widths(), [0, 0, 0, 6]);
        assert_eq!(
            inline_style.border_styles(),
            [
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::None,
                NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
            ]
        );
        assert_eq!(
            inline_style.border_colors(),
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                blue
            ]
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
    fn text_decoration_declaration_parser_accepts_css_wide_resets_and_line_values() {
        assert_eq!(
            parse_text_decoration_declaration("ReVeRt-LaYeR"),
            Some(NativeTextDecorationDeclaration::RevertLayer)
        );
        assert_eq!(
            parse_text_decoration_declaration("InHeRiT"),
            Some(NativeTextDecorationDeclaration::Inherit)
        );
        assert_eq!(
            parse_text_decoration_declaration("InItIaL"),
            Some(NativeTextDecorationDeclaration::Initial)
        );
        assert_eq!(
            parse_text_decoration_declaration("UnSeT"),
            Some(NativeTextDecorationDeclaration::Unset)
        );
        assert_eq!(
            parse_text_decoration_declaration("ReVeRt"),
            Some(NativeTextDecorationDeclaration::Revert)
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
        assert_eq!(
            parse_declarations("text-decoration-line: inherit").text_decoration,
            Some(NativeTextDecorationDeclaration::Inherit)
        );
        assert_eq!(
            parse_declarations("text-decoration: initial").text_decoration,
            Some(NativeTextDecorationDeclaration::Initial)
        );
    }

    #[test]
    fn text_decoration_css_wide_resets_follow_parent_and_initial() {
        let document = NativeDocument::parse(
            "<style>.line { display:block; } #parent { text-decoration: underline overline; } #initial { text-decoration-line: initial; } #inherit { text-decoration-line: inherit; } #unset { text-decoration: unset; } #revert { text-decoration: revert; } #invalid { text-decoration-line: overline; text-decoration-line: invalid; } #important { text-decoration-line: underline !important; } #important { text-decoration-line: none; } #root-initial { text-decoration-line: initial; }</style><div id='parent' class='line'><span id='initial' class='line'>Initial</span><span id='inherit' class='line'>Inherit</span><span id='unset' class='line'>Unset</span><span id='revert' class='line'>Revert</span><span id='invalid' class='line'>Invalid</span><span id='important' class='line'>Important</span></div><div id='root-initial' class='line'>Root initial</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let line_for = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
                .text_decoration()
        };
        let parent = TextDecorationValue::new(true, true, false);
        assert_eq!(line_for("parent"), parent);
        assert_eq!(line_for("initial"), TextDecorationValue::none());
        assert_eq!(line_for("inherit"), parent);
        assert_eq!(line_for("unset"), parent);
        assert_eq!(line_for("revert"), parent);
        assert_eq!(
            line_for("invalid"),
            TextDecorationValue::new(false, true, false)
        );
        assert_eq!(
            line_for("important"),
            TextDecorationValue::new(true, false, false)
        );
        assert_eq!(line_for("root-initial"), TextDecorationValue::none());
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
    fn text_presentation_important_parser_tracks_markers_and_invalid_preservation() {
        let declarations = parse_declarations(
            "white-space: pre !IMPORTANT; text-align: center !important; text-align-last: right !important; text-justify: inter-word !important; direction: rtl !important; text-decoration: underline !important; text-decoration-style: dashed !important; text-decoration-skip-ink: none !important; text-decoration-skip-spaces: all !important; text-decoration-thickness: 2px !important; text-underline-offset: -2px !important; text-transform: uppercase !important; font-weight: bold !important; font-style: italic !important; word-break: break-all !important; text-overflow: ellipsis !important; vertical-align: middle !important; text-indent: 4px !important; word-spacing: 2px !important; letter-spacing: 3px !important; line-height: 20px !important",
        );
        let importance = declarations.text_importance;
        assert!(importance.white_space);
        assert!(importance.text_align);
        assert!(importance.text_align_last);
        assert!(importance.text_justify);
        assert!(importance.direction);
        assert!(importance.text_decoration);
        assert!(importance.text_decoration_style);
        assert!(importance.text_decoration_skip_ink);
        assert!(importance.text_decoration_skip_spaces);
        assert!(importance.text_decoration_thickness);
        assert!(importance.text_underline_offset);
        assert!(importance.text_transform);
        assert!(importance.font_weight);
        assert!(importance.font_style);
        assert!(importance.word_break);
        assert!(importance.text_overflow);
        assert!(importance.vertical_align);
        assert!(importance.text_indent);
        assert!(importance.word_spacing);
        assert!(importance.letter_spacing);
        assert!(importance.line_height);

        let preserved = parse_declarations(
            "white-space: pre !important; white-space: invalid !important; text-decoration: underline !important; text-decoration: invalid !important; line-height: 20px !important; line-height: invalid !important",
        );
        assert_eq!(
            preserved.white_space,
            Some(InheritedTextDeclaration::Value(WhiteSpaceValue::Pre))
        );
        assert_eq!(
            preserved.text_decoration,
            Some(NativeTextDecorationDeclaration::Value(
                TextDecorationValue::new(true, false, false)
            ))
        );
        assert_eq!(
            preserved.line_height,
            Some(InheritedTextDeclaration::Value(20))
        );
        assert!(preserved.text_importance.white_space);
        assert!(preserved.text_importance.text_decoration);
        assert!(preserved.text_importance.line_height);
    }

    #[test]
    fn stylesheet_cascade_resolves_text_presentation_important_priority_and_rollback() {
        let document = NativeDocument::parse(
            "<style>.line { display:block; width:32px; } @layer base { #winner { white-space:pre !important; text-align:center !important; text-transform:uppercase !important; text-decoration:underline !important; text-decoration-style:dashed !important; line-height:24px !important; } #rollback { text-transform:uppercase !important; text-decoration-style:dashed !important; } } @layer theme { #winner { white-space:nowrap !important; text-align:right !important; text-transform:lowercase !important; text-decoration:overline !important; text-decoration-style:dotted !important; line-height:12px !important; } #rollback { text-transform:revert-layer !important; text-decoration-style:revert-layer !important; } } #winner { white-space:normal !important; text-align:left !important; text-transform:none !important; text-decoration:none !important; text-decoration-style:solid !important; line-height:8px !important; } #normal { white-space:pre; text-align:right; text-transform:lowercase; line-height:8px; } </style><div id='winner' class='line'>A B</div><div id='rollback' class='line'>A</div><div id='normal' class='line'>A</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let style_for = |id| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
        };
        let winner = style_for("winner");
        assert_eq!(winner.white_space(), WhiteSpaceValue::Pre);
        assert_eq!(winner.text_align(), TextAlignValue::Center);
        assert_eq!(winner.text_transform(), TextTransformValue::Uppercase);
        assert_eq!(
            winner.text_decoration(),
            TextDecorationValue::new(true, false, false)
        );
        assert_eq!(
            winner.text_decoration_style(),
            NativeTextDecorationStyle::Dashed
        );
        assert_eq!(winner.line_height(), Some(24));

        let rollback = style_for("rollback");
        assert_eq!(rollback.text_transform(), TextTransformValue::Uppercase);
        assert_eq!(
            rollback.text_decoration_style(),
            NativeTextDecorationStyle::Dashed
        );

        let normal = style_for("normal");
        assert_eq!(normal.white_space(), WhiteSpaceValue::Pre);
        assert_eq!(normal.text_align(), TextAlignValue::Right);
        assert_eq!(normal.text_transform(), TextTransformValue::Lowercase);
        assert_eq!(normal.line_height(), Some(8));
    }

    #[test]
    fn flex_gap_important_parser_tracks_markers_and_invalid_preservation() {
        let declarations = parse_declarations(
            "justify-content: center !IMPORTANT; place-content: space-around flex-end !important; align-items: center !important; align-self: flex-end !important; align-content: space-between !important; flex-direction: column !important; flex-wrap: wrap !important; flex-flow: row-reverse wrap-reverse !important; order: -3 !important; flex: 2 3 12px !important; flex-grow: 4 !important; flex-shrink: 5 !important; flex-basis: 16px !important; gap: 2px 3px !important; row-gap: 4px !important; column-gap: 5px !important",
        );
        assert_eq!(
            declarations.justify_content,
            Some(JustifyContentDeclaration::Value(
                JustifyContentValue::FlexEnd
            ))
        );
        assert_eq!(
            declarations.align_content,
            Some(AlignContentDeclaration::Value(
                AlignContentValue::SpaceBetween
            ))
        );
        assert_eq!(
            declarations.flex_direction,
            Some(FlexDirectionDeclaration::Value(
                FlexDirectionValue::RowReverse
            ))
        );
        assert_eq!(
            declarations.flex_wrap,
            Some(FlexWrapDeclaration::Value(FlexWrapValue::WrapReverse))
        );
        assert_eq!(
            declarations.order,
            Some(FlexItemOrderDeclaration::Value(NativeOrderValue(-3)))
        );
        assert_eq!(declarations.flex_grow, Some(FlexGrowDeclaration::Value(4)));
        assert_eq!(
            declarations.flex_shrink,
            Some(FlexShrinkDeclaration::Value(5))
        );
        assert_eq!(
            declarations.flex_basis,
            Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(16)))
        );
        assert_eq!(
            declarations.gap,
            Some(GapShorthandDeclaration::Value(NativeGapValue {
                row: 2,
                column: 3,
            }))
        );
        assert_eq!(
            declarations.row_gap,
            Some(GapComponentDeclaration::Value(4))
        );
        assert_eq!(
            declarations.column_gap,
            Some(GapComponentDeclaration::Value(5))
        );
        assert!(declarations.flex_importance.justify_content);
        assert!(declarations.flex_importance.align_items);
        assert!(declarations.flex_importance.align_self);
        assert!(declarations.flex_importance.align_content);
        assert!(declarations.flex_importance.flex_direction);
        assert!(declarations.flex_importance.flex_wrap);
        assert!(declarations.flex_importance.order);
        assert!(declarations.flex_importance.flex_grow);
        assert!(declarations.flex_importance.flex_shrink);
        assert!(declarations.flex_importance.flex_basis);
        assert!(declarations.gap_important);
        assert!(declarations.row_gap_important);
        assert!(declarations.column_gap_important);

        let preserved = parse_declarations(
            "place-content: space-around flex-end !important; place-content: unsupported !important; flex-flow: column wrap !important; flex-flow: column wrap wrap !important; flex: 2 3 12px !important; flex: 1 2 3% !important; gap: 2px 3px !important; gap: 1px 2px 3px !important; row-gap: 4px !important; row-gap: 1px 2px !important; column-gap: 5px !important; column-gap: revert-layer 1px !important",
        );
        assert_eq!(
            preserved.justify_content,
            Some(JustifyContentDeclaration::Value(
                JustifyContentValue::FlexEnd
            ))
        );
        assert_eq!(
            preserved.align_content,
            Some(AlignContentDeclaration::Value(
                AlignContentValue::SpaceAround
            ))
        );
        assert_eq!(
            preserved.flex_direction,
            Some(FlexDirectionDeclaration::Value(FlexDirectionValue::Column))
        );
        assert_eq!(
            preserved.flex_wrap,
            Some(FlexWrapDeclaration::Value(FlexWrapValue::Wrap))
        );
        assert_eq!(preserved.flex_grow, Some(FlexGrowDeclaration::Value(2)));
        assert_eq!(preserved.flex_shrink, Some(FlexShrinkDeclaration::Value(3)));
        assert_eq!(
            preserved.flex_basis,
            Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(12)))
        );
        assert_eq!(
            preserved.gap,
            Some(GapShorthandDeclaration::Value(NativeGapValue {
                row: 2,
                column: 3,
            }))
        );
        assert_eq!(preserved.row_gap, Some(GapComponentDeclaration::Value(4)));
        assert_eq!(
            preserved.column_gap,
            Some(GapComponentDeclaration::Value(5))
        );
        assert!(preserved.flex_importance.justify_content);
        assert!(preserved.flex_importance.align_content);
        assert!(preserved.flex_importance.flex_direction);
        assert!(preserved.flex_importance.flex_wrap);
        assert!(preserved.flex_importance.flex_grow);
        assert!(preserved.flex_importance.flex_shrink);
        assert!(preserved.flex_importance.flex_basis);
        assert!(preserved.gap_important);
        assert!(preserved.row_gap_important);
        assert!(preserved.column_gap_important);
    }

    #[test]
    fn stylesheet_cascade_resolves_flex_gap_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { place-content: space-around flex-end !important; align-items: center !important; align-self: flex-end !important; flex-flow: column wrap !important; order: -3 !important; flex: 2 3 12px !important; gap: 4px 6px !important; }
                #rollback { place-content: revert-layer !important; align-items: revert-layer !important; align-self: revert-layer !important; flex-flow: revert-layer !important; order: revert-layer !important; flex: revert-layer !important; gap: revert-layer !important; }
                #invalid { place-content: space-around flex-end !important; align-items: center !important; align-self: flex-end !important; flex-flow: column wrap !important; order: -3 !important; flex: 2 3 12px !important; gap: 4px 6px !important; }
                #normal { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: -1; flex: 1 1 8px; gap: 1px 2px; }
                #inline-normal { place-content: space-around flex-end !important; flex-flow: column wrap !important; flex: 2 3 12px !important; gap: 4px 6px !important; }
            }
            @layer theme {
                #important { place-content: center flex-start !important; align-items: flex-end !important; align-self: flex-start !important; flex-flow: row-reverse nowrap !important; order: 8 !important; flex: 4 5 20px !important; gap: 9px 10px !important; }
                #rollback { place-content: center flex-end !important; align-items: flex-end !important; align-self: center !important; flex-flow: row-reverse wrap-reverse !important; order: 8 !important; flex: 4 5 20px !important; gap: 9px 10px !important; }
                #invalid { place-content: unsupported !important; align-items: unsupported !important; align-self: unsupported !important; flex-flow: column wrap wrap !important; order: 1025 !important; flex: 1 2 3% !important; gap: 1px 2px 3px !important; }
                #normal { place-content: center center; align-items: center; align-self: center; flex-flow: column wrap; order: 2; flex: 2 2 10px; gap: 3px 4px; }
            }
            @layer top {
                #rollback { place-content: flex-end flex-start !important; align-items: flex-start !important; align-self: flex-start !important; flex-flow: column-reverse nowrap !important; order: 12 !important; flex: 6 7 24px !important; gap: 12px 13px !important; }
            }
            #important { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: 1; flex: 1 1 1px; gap: 1px 1px; }
            #rollback { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: 1; flex: 1 1 1px; gap: 1px 1px; }
            #invalid { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: 1; flex: 1 1 1px; gap: 1px 1px; }
            #normal { place-content: space-between flex-end; align-items: flex-end; align-self: flex-end; flex-flow: row-reverse wrap-reverse; order: 4; flex: 3 4 12px; gap: 7px 8px; }
            #inline-normal { place-content: flex-start flex-start; flex-flow: row nowrap; flex: 1 1 1px; gap: 1px 1px; }
            #inline-important { place-content: flex-end flex-end !important; flex-flow: row-reverse wrap-reverse !important; flex: 3 4 12px !important; gap: 7px 8px !important; }
        "#
        .into(),
        ])
        .unwrap();
        let important = node("<div id='important'>Important</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let normal = node("<div id='normal'>Normal</div>");
        let inline_normal = node(
            "<div id='inline-normal' style='place-content:center center;flex-flow:column wrap;flex:5 6 18px;gap:11px 12px'>Inline normal</div>",
        );
        let inline_important = node(
            "<div id='inline-important' style='place-content:space-between flex-end !important;flex-flow:column-reverse wrap-reverse !important;flex:5 6 18px !important;gap:11px 12px !important'>Inline important</div>",
        );

        let assert_values = |element: &NativeNode,
                             justify_content,
                             align_items,
                             align_self,
                             align_content,
                             flex_direction,
                             flex_wrap,
                             order,
                             flex_grow,
                             flex_shrink,
                             flex_basis,
                             row_gap,
                             column_gap| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.justify_content(), justify_content);
            assert_eq!(style.align_items(), align_items);
            assert_eq!(style.align_self(), align_self);
            assert_eq!(style.align_content(), align_content);
            assert_eq!(style.flex_direction(), flex_direction);
            assert_eq!(style.flex_wrap(), flex_wrap);
            assert_eq!(style.flex_item_order(), order);
            assert_eq!(style.flex_grow(), flex_grow);
            assert_eq!(style.flex_shrink(), flex_shrink);
            assert_eq!(style.flex_basis(), flex_basis);
            assert_eq!(style.row_gap(), row_gap);
            assert_eq!(style.column_gap(), column_gap);
        };

        assert_values(
            &important,
            JustifyContentValue::FlexEnd,
            AlignItemsValue::Center,
            AlignSelfValue::FlexEnd,
            AlignContentValue::SpaceAround,
            FlexDirectionValue::Column,
            FlexWrapValue::Wrap,
            NativeOrderValue(-3),
            2,
            3,
            FlexBasisValue::Length(12),
            4,
            6,
        );
        assert_values(
            &rollback,
            JustifyContentValue::FlexEnd,
            AlignItemsValue::FlexEnd,
            AlignSelfValue::Center,
            AlignContentValue::Center,
            FlexDirectionValue::RowReverse,
            FlexWrapValue::WrapReverse,
            NativeOrderValue(8),
            4,
            5,
            FlexBasisValue::Length(20),
            9,
            10,
        );
        assert_values(
            &invalid,
            JustifyContentValue::FlexEnd,
            AlignItemsValue::Center,
            AlignSelfValue::FlexEnd,
            AlignContentValue::SpaceAround,
            FlexDirectionValue::Column,
            FlexWrapValue::Wrap,
            NativeOrderValue(-3),
            2,
            3,
            FlexBasisValue::Length(12),
            4,
            6,
        );
        assert_values(
            &normal,
            JustifyContentValue::FlexEnd,
            AlignItemsValue::FlexEnd,
            AlignSelfValue::FlexEnd,
            AlignContentValue::SpaceBetween,
            FlexDirectionValue::RowReverse,
            FlexWrapValue::WrapReverse,
            NativeOrderValue(4),
            3,
            4,
            FlexBasisValue::Length(12),
            7,
            8,
        );
        assert_values(
            &inline_normal,
            JustifyContentValue::FlexEnd,
            AlignItemsValue::FlexStart,
            AlignSelfValue::Auto,
            AlignContentValue::SpaceAround,
            FlexDirectionValue::Column,
            FlexWrapValue::Wrap,
            NativeOrderValue(0),
            2,
            3,
            FlexBasisValue::Length(12),
            4,
            6,
        );
        assert_values(
            &inline_important,
            JustifyContentValue::FlexEnd,
            AlignItemsValue::FlexStart,
            AlignSelfValue::Auto,
            AlignContentValue::SpaceBetween,
            FlexDirectionValue::ColumnReverse,
            FlexWrapValue::WrapReverse,
            NativeOrderValue(0),
            5,
            6,
            FlexBasisValue::Length(18),
            11,
            12,
        );
    }

    #[test]
    fn dimension_important_parser_tracks_markers_and_invalid_preservation() {
        let declarations = parse_declarations(
            "width: 24px !IMPORTANT; height: 30px !important; min-width: 8px !important; max-width: 64px !important; min-height: 10px !important; max-height: 80px !important",
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
            declarations.max_width,
            Some(LocalCascadeDeclaration::Value(64))
        );
        assert_eq!(
            declarations.min_height,
            Some(LocalCascadeDeclaration::Value(10))
        );
        assert_eq!(
            declarations.max_height,
            Some(LocalCascadeDeclaration::Value(80))
        );
        assert_eq!(
            declarations.dimension_importance,
            NativeDimensionDeclarationImportance {
                width: true,
                height: true,
                min_width: true,
                max_width: true,
                min_height: true,
                max_height: true,
            }
        );

        let preserved = parse_declarations(
            "width:24px !important;width:bad !important;height:30px !important;height:1px 2px !important;min-width:8px !important;min-width:-1px !important;max-width:64px !important;max-width:50% !important;min-height:10px !important;min-height:auto !important;max-height:80px !important;max-height:revert-layer 1px !important",
        );
        assert_eq!(preserved.width, Some(LocalCascadeDeclaration::Value(24)));
        assert_eq!(preserved.height, Some(LocalCascadeDeclaration::Value(30)));
        assert_eq!(preserved.min_width, Some(LocalCascadeDeclaration::Value(8)));
        assert_eq!(
            preserved.max_width,
            Some(LocalCascadeDeclaration::Value(64))
        );
        assert_eq!(
            preserved.min_height,
            Some(LocalCascadeDeclaration::Value(10))
        );
        assert_eq!(
            preserved.max_height,
            Some(LocalCascadeDeclaration::Value(80))
        );
        assert_eq!(
            preserved.dimension_importance,
            NativeDimensionDeclarationImportance {
                width: true,
                height: true,
                min_width: true,
                max_width: true,
                min_height: true,
                max_height: true,
            }
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_dimension_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { width:24px !important; height:12px !important; min-width:4px !important; max-width:80px !important; min-height:6px !important; max-height:90px !important; }
                #rollback { width:30px !important; height:18px !important; min-width:8px !important; max-width:70px !important; min-height:10px !important; max-height:60px !important; }
                #rollback { width:revert-layer !important; height:revert-layer !important; min-width:revert-layer !important; max-width:revert-layer !important; min-height:revert-layer !important; max-height:revert-layer !important; }
                #invalid { width:40px !important; width:bad !important; height:20px !important; height:1px 2px !important; min-width:12px !important; min-width:-1px !important; max-width:60px !important; max-width:50% !important; min-height:14px !important; min-height:auto !important; max-height:50px !important; max-height:revert-layer 1px !important; }
                #normal { width:10px; height:10px; min-width:2px; max-width:20px; min-height:2px; max-height:20px; }
            }
            @layer theme {
                #important { width:48px !important; height:20px !important; min-width:16px !important; max-width:100px !important; min-height:10px !important; max-height:110px !important; }
                #rollback { width:36px !important; height:22px !important; min-width:12px !important; max-width:90px !important; min-height:8px !important; max-height:80px !important; }
                #normal { width:20px; height:20px; min-width:4px; max-width:40px; min-height:4px; max-height:40px; }
            }
            #important { width:64px !important; height:28px !important; min-width:24px !important; max-width:120px !important; min-height:14px !important; max-height:130px !important; }
            #rollback { width:72px !important; height:32px !important; min-width:28px !important; max-width:140px !important; min-height:16px !important; max-height:150px !important; }
            #invalid { width:80px; height:40px; min-width:32px; max-width:160px; min-height:18px; max-height:170px; }
            #normal { width:32px; height:32px; min-width:6px; max-width:60px; min-height:6px; max-height:60px; }
            #inline { width:64px !important; height:28px !important; min-width:24px !important; max-width:120px !important; min-height:14px !important; max-height:130px !important; }
        "#
        .into(),
        ])
        .unwrap();
        let important = node("<div id='important'>Important</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let normal = node("<div id='normal'>Normal</div>");
        let inline = node(
            "<div id='inline' style='width:91px !important;height:33px !important;min-width:31px !important;max-width:141px !important;min-height:17px !important;max-height:151px !important'>Inline</div>",
        );

        let assert_dimensions = |element: &NativeNode, expected: [Option<u32>; 6]| {
            let style = stylesheet.computed_for(element);
            assert_eq!(
                [
                    style.width(),
                    style.height(),
                    style.min_width(),
                    style.max_width(),
                    style.min_height(),
                    style.max_height(),
                ],
                expected
            );
        };

        assert_dimensions(
            &important,
            [Some(24), Some(12), Some(4), Some(80), Some(6), Some(90)],
        );
        assert_dimensions(
            &rollback,
            [Some(36), Some(22), Some(12), Some(90), Some(8), Some(80)],
        );
        assert_dimensions(
            &invalid,
            [Some(40), Some(20), Some(12), Some(60), Some(14), Some(50)],
        );
        assert_dimensions(
            &normal,
            [Some(32), Some(32), Some(6), Some(60), Some(6), Some(60)],
        );
        assert_dimensions(
            &inline,
            [Some(91), Some(33), Some(31), Some(141), Some(17), Some(151)],
        );
    }

    #[test]
    fn box_model_important_parser_tracks_markers_and_invalid_preservation() {
        let declarations = parse_declarations(
            "box-sizing: border-box !IMPORTANT; padding: 1px 2px 3px 4px !important; padding-left: 5px; margin: auto 2px 3px 4px !important; margin-bottom: auto !IMPORTANT",
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
            declarations.box_model_importance.padding,
            [true, true, true, false]
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
        assert_eq!(
            declarations.box_model_importance.margin,
            [true, true, true, true]
        );
        assert_eq!(
            declarations.box_sizing,
            Some(LocalCascadeDeclaration::Value(NativeBoxSizing::BorderBox))
        );
        assert!(declarations.box_model_importance.box_sizing);

        let preserved = parse_declarations(
            "box-sizing:border-box !important;box-sizing:auto !important;padding:4px !important;padding:bad !important;padding-right:9px !important;padding-right:-1px !important;margin:auto 2px 3px 4px !important;margin:-1px !important;margin-top:6px !important;margin-top:50% !important",
        );
        assert_eq!(
            preserved.padding,
            [
                Some(LocalCascadeDeclaration::Value(4)),
                Some(LocalCascadeDeclaration::Value(9)),
                Some(LocalCascadeDeclaration::Value(4)),
                Some(LocalCascadeDeclaration::Value(4)),
            ]
        );
        assert_eq!(
            preserved.box_model_importance.padding,
            [true, true, true, true]
        );
        assert_eq!(
            preserved.margin,
            [
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(6))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(2))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(3))),
                Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(4))),
            ]
        );
        assert_eq!(
            preserved.box_model_importance.margin,
            [true, true, true, true]
        );
        assert_eq!(
            preserved.box_sizing,
            Some(LocalCascadeDeclaration::Value(NativeBoxSizing::BorderBox))
        );
        assert!(preserved.box_model_importance.box_sizing);
    }

    #[test]
    fn stylesheet_cascade_resolves_box_model_important_priority_and_rollback() {
        let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { box-sizing:content-box !important; padding:1px 2px 3px 4px !important; padding-left:5px !important; margin:auto 2px 3px 4px !important; margin-bottom:6px !important; }
                #rollback { box-sizing:content-box !important; padding:1px 2px 3px 4px !important; margin:1px 2px 3px 4px !important; }
                #rollback { box-sizing:revert-layer !important; padding:revert-layer !important; margin:revert-layer !important; }
                #invalid { box-sizing:border-box !important; box-sizing:auto !important; padding:4px !important; padding:bad !important; margin:5px !important; margin:-1px !important; }
                #normal { box-sizing:content-box; padding:1px; margin:1px; }
            }
            @layer theme {
                #important { box-sizing:border-box !important; padding:5px 6px 7px 8px !important; margin:5px 6px 7px 8px !important; }
                #rollback { box-sizing:border-box !important; padding:6px 7px 8px 9px !important; margin:6px 7px 8px 9px !important; }
                #normal { box-sizing:border-box; padding:2px; margin:2px; }
            }
            #important { box-sizing:border-box !important; padding:9px !important; margin:9px !important; }
            #rollback { box-sizing:content-box !important; padding:10px !important; margin:10px !important; }
            #normal { box-sizing:content-box; padding:3px; margin:3px; }
            #inline { box-sizing:content-box !important; padding:11px !important; margin:11px !important; }"#
            .into(),
        ])
        .unwrap();
        let important = node("<div id='important'>Important</div>");
        let rollback = node("<div id='rollback'>Rollback</div>");
        let invalid = node("<div id='invalid'>Invalid</div>");
        let normal = node("<div id='normal'>Normal</div>");
        let inline = node(
            "<div id='inline' style='box-sizing:border-box !important;padding:12px !important;margin:auto !important'>Inline</div>",
        );

        let assert_box_model = |element: &NativeNode,
                                border_box: bool,
                                padding: NativeBoxEdges,
                                margin: NativeBoxEdges,
                                margin_auto: NativeAutoEdges| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.is_border_box(), border_box);
            assert_eq!(style.padding(), padding);
            assert_eq!(style.margin(), margin);
            assert_eq!(style.margin_auto(), margin_auto);
        };

        assert_box_model(
            &important,
            false,
            NativeBoxEdges {
                top: 1,
                right: 2,
                bottom: 3,
                left: 5,
            },
            NativeBoxEdges {
                top: 0,
                right: 2,
                bottom: 6,
                left: 4,
            },
            NativeAutoEdges {
                top: true,
                right: false,
                bottom: false,
                left: false,
            },
        );
        assert_box_model(
            &rollback,
            true,
            NativeBoxEdges {
                top: 6,
                right: 7,
                bottom: 8,
                left: 9,
            },
            NativeBoxEdges {
                top: 6,
                right: 7,
                bottom: 8,
                left: 9,
            },
            NativeAutoEdges::default(),
        );
        assert_box_model(
            &invalid,
            true,
            NativeBoxEdges::from_values([Some(4); 4]),
            NativeBoxEdges::from_values([Some(5); 4]),
            NativeAutoEdges::default(),
        );
        assert_box_model(
            &normal,
            false,
            NativeBoxEdges::from_values([Some(3); 4]),
            NativeBoxEdges::from_values([Some(3); 4]),
            NativeAutoEdges::default(),
        );
        assert_box_model(
            &inline,
            true,
            NativeBoxEdges::from_values([Some(12); 4]),
            NativeBoxEdges::default(),
            NativeAutoEdges {
                top: true,
                right: true,
                bottom: true,
                left: true,
            },
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_box_model_inherit_for_physical_and_logical_edges() {
        assert_eq!(
            parse_local_box_edges("InHeRiT"),
            Some([LocalCascadeDeclaration::Inherit; 4])
        );
        assert_eq!(
            parse_local_box_edge_pair("inherit"),
            Some([LocalCascadeDeclaration::Inherit; 2])
        );
        assert_eq!(
            parse_local_padding_declaration("INHERIT"),
            Some(LocalCascadeDeclaration::Inherit)
        );
        assert_eq!(
            parse_local_margin_declaration("inherit"),
            Some(LocalCascadeDeclaration::Inherit)
        );
        assert_eq!(
            parse_local_box_sizing_declaration("InHeRiT"),
            Some(LocalCascadeDeclaration::Inherit)
        );
        assert_eq!(parse_local_box_edges("inherit 1px"), None);
        assert_eq!(parse_local_margin_edge_pair("auto inherit"), None);

        let document = NativeDocument::parse(
            r#"<style>
            #physical-parent { padding:2px 4px 6px 8px; margin:auto 3px 5px 7px; box-sizing:border-box; }
            #physical-child { padding:inherit; margin:INHERIT; box-sizing:InHeRiT; }
            #logical-parent { direction:ltr; padding-inline:11px 13px; margin-inline:auto 17px; }
            #logical-child { direction:rtl; padding-inline-start:inherit; padding-inline-end:INHERIT; margin-inline-start:inherit; margin-inline-end:inherit; }
            #important-parent { padding:12px 13px 14px 15px; margin:auto 16px 17px 18px; box-sizing:border-box; }
            #important-child { padding:1px; padding:inherit !important; margin:2px; margin:inherit !important; box-sizing:content-box; box-sizing:inherit !important; }
            #rollback-parent { padding:19px; margin:20px; box-sizing:border-box; }
            @layer base { #rollback-child { padding:inherit; margin:inherit; box-sizing:inherit; } }
            @layer theme { #rollback-child { padding:1px; padding:revert-layer; margin:2px; margin:revert-layer; box-sizing:content-box; box-sizing:revert-layer; } }
            #root-inherit { padding:inherit; margin:inherit; box-sizing:inherit; }
            </style>
            <div id='physical-parent'><div id='physical-child'>Physical</div></div>
            <div id='logical-parent'><div id='logical-child'>Logical</div></div>
            <div id='important-parent'><div id='important-child'>Important</div></div>
            <div id='rollback-parent'><div id='rollback-child'>Rollback</div></div>
            <div id='root-inherit'>Root</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
        assert!(document.diagnostics().iter().all(|diagnostic| {
            !matches!(
                diagnostic.code,
                NativeDiagnosticCode::UnsupportedCssProperty
                    | NativeDiagnosticCode::UnsupportedCssValue
            )
        }));

        let style = |id: &str| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
        };
        let physical_parent = style("physical-parent");
        let physical_child = style("physical-child");
        assert!(physical_parent.is_border_box());
        assert_eq!(physical_child.padding(), physical_parent.padding());
        assert_eq!(physical_child.margin(), physical_parent.margin());
        assert_eq!(physical_child.margin_auto(), physical_parent.margin_auto());
        assert!(physical_child.is_border_box());

        let logical_child = style("logical-child");
        assert_eq!(
            logical_child.padding(),
            NativeBoxEdges {
                top: 0,
                right: 11,
                bottom: 0,
                left: 13,
            }
        );
        assert_eq!(
            logical_child.margin(),
            NativeBoxEdges {
                top: 0,
                right: 0,
                bottom: 0,
                left: 17,
            }
        );
        assert!(logical_child.margin_auto().right());
        assert!(!logical_child.margin_auto().left());

        let important_parent = style("important-parent");
        let important_child = style("important-child");
        assert_eq!(important_child.padding(), important_parent.padding());
        assert_eq!(important_child.margin(), important_parent.margin());
        assert_eq!(
            important_child.margin_auto(),
            important_parent.margin_auto()
        );
        assert!(important_child.is_border_box());

        let rollback_parent = style("rollback-parent");
        let rollback_child = style("rollback-child");
        assert_eq!(rollback_child.padding(), rollback_parent.padding());
        assert_eq!(rollback_child.margin(), rollback_parent.margin());
        assert!(rollback_child.is_border_box());

        let root = style("root-inherit");
        assert_eq!(root.padding(), NativeBoxEdges::default());
        assert_eq!(root.margin(), NativeBoxEdges::default());
        assert_eq!(root.margin_auto(), NativeAutoEdges::default());
        assert!(!root.is_border_box());
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
    fn inherited_text_declaration_parsers_accept_standalone_css_wide_keywords() {
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
        assert_eq!(
            parse_font_weight_declaration("revert"),
            Some(InheritedTextDeclaration::Revert)
        );
        assert_eq!(
            parse_font_style_declaration("INITIAL"),
            Some(InheritedTextDeclaration::Initial)
        );
        assert_eq!(
            parse_word_break_declaration("unset"),
            Some(InheritedTextDeclaration::Unset)
        );
        assert_eq!(
            parse_text_transform_declaration("inherit"),
            Some(InheritedTextDeclaration::Inherit)
        );
        assert_eq!(parse_text_transform_declaration("uppercase initial"), None);
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
    fn inherited_text_spacing_declaration_parsers_accept_standalone_css_wide_keywords() {
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
        assert_eq!(
            parse_word_spacing_declaration("revert"),
            Some(InheritedTextDeclaration::Revert)
        );
        assert_eq!(
            parse_letter_spacing_declaration("unset"),
            Some(InheritedTextDeclaration::Unset)
        );
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
    fn inherited_text_css_wide_keywords_resolve_parent_initial_and_terminal_fallbacks() {
        let document = NativeDocument::parse(
            r#"<style>
            #parent { white-space:pre; line-height:28px; text-transform:uppercase; font-weight:bold; font-style:italic; word-break:break-all; vertical-align:middle; word-spacing:12px; letter-spacing:13px; }
            #inherit { white-space:inherit; line-height:inherit; text-transform:inherit; font-weight:inherit; font-style:inherit; word-break:inherit; vertical-align:inherit; word-spacing:inherit; letter-spacing:inherit; }
            #unset { white-space:unset; line-height:unset; text-transform:unset; font-weight:unset; font-style:unset; word-break:unset; vertical-align:unset; word-spacing:unset; letter-spacing:unset; }
            #revert { white-space:ReVeRt; line-height:ReVeRt; text-transform:ReVeRt; font-weight:ReVeRt; font-style:ReVeRt; word-break:ReVeRt; vertical-align:ReVeRt; word-spacing:ReVeRt; letter-spacing:ReVeRt; }
            #initial { white-space:initial; line-height:initial; text-transform:initial; font-weight:initial; font-style:initial; word-break:initial; vertical-align:initial; word-spacing:initial; letter-spacing:initial; }
            #terminal { white-space:pre; white-space:initial; line-height:28px; line-height:initial; text-transform:uppercase; text-transform:initial; font-weight:bold; font-weight:initial; font-style:italic; font-style:initial; word-break:break-all; word-break:initial; vertical-align:middle; vertical-align:initial; word-spacing:12px; word-spacing:initial; letter-spacing:13px; letter-spacing:initial; }
            #invalid { white-space:pre; white-space:break-spaces; line-height:28px; line-height:0px; text-transform:uppercase; text-transform:capitalize; font-weight:bold; font-weight:500; font-style:italic; font-style:oblique; word-break:break-all; word-break:keep-all; vertical-align:middle; vertical-align:sub; word-spacing:12px; word-spacing:-1px; letter-spacing:13px; letter-spacing:-1px; }
            </style>
            <div id='parent'><span id='inherit'>inherit</span><span id='unset'>unset</span><span id='revert'>revert</span><span id='initial'>initial</span><span id='terminal'>terminal</span><span id='invalid'>invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

        let values = |id| {
            let style = document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
            (
                style.white_space(),
                style.line_height(),
                style.text_transform(),
                style.font_weight(),
                style.font_style(),
                style.word_break(),
                style.vertical_align(),
                style.word_spacing(),
                style.letter_spacing(),
            )
        };
        let inherited = (
            WhiteSpaceValue::Pre,
            Some(28),
            TextTransformValue::Uppercase,
            FontWeightValue::Bold,
            FontStyleValue::Italic,
            WordBreakValue::BreakAll,
            VerticalAlignValue::Middle,
            12,
            13,
        );
        let initial = (
            WhiteSpaceValue::Normal,
            None,
            TextTransformValue::None,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            WordBreakValue::Normal,
            VerticalAlignValue::Baseline,
            0,
            0,
        );

        assert_eq!(values("parent"), inherited);
        assert_eq!(values("inherit"), inherited);
        assert_eq!(values("unset"), inherited);
        assert_eq!(values("revert"), inherited);
        assert_eq!(values("initial"), initial);
        assert_eq!(values("terminal"), initial);
        assert_eq!(values("invalid"), inherited);
    }

    #[test]
    fn inherited_alignment_css_wide_keywords_resolve_parent_initial_and_terminal_fallbacks() {
        let document = NativeDocument::parse(
            r#"<style>
            #parent { text-align:right; text-align-last:justify; text-justify:inter-word; direction:rtl; }
            #inherit { text-align:inherit; text-align-last:inherit; text-justify:inherit; direction:inherit; }
            #unset { text-align:unset; text-align-last:unset; text-justify:unset; direction:unset; }
            #revert { text-align:ReVeRt; text-align-last:ReVeRt; text-justify:ReVeRt; direction:ReVeRt; }
            #initial { text-align:initial; text-align-last:initial; text-justify:initial; direction:initial; }
            #terminal { text-align:center; text-align:initial; text-align-last:end; text-align-last:initial; text-justify:none; text-justify:initial; direction:ltr; direction:initial; }
            #invalid { text-align:right; text-align:match-parent; text-align-last:justify; text-align-last:match-parent; text-justify:inter-word; text-justify:inter-character; direction:rtl; direction:vertical-rl; }
            </style>
            <div id='parent'><span id='inherit'>inherit</span><span id='unset'>unset</span><span id='revert'>revert</span><span id='initial'>initial</span><span id='terminal'>terminal</span><span id='invalid'>invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

        let values = |id| {
            let style = document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
            (
                style.text_align(),
                style.text_align_last(),
                style.text_justify(),
                style.direction(),
            )
        };
        let inherited = (
            TextAlignValue::Right,
            TextAlignLastValue::Justify,
            TextJustifyValue::InterWord,
            DirectionValue::Rtl,
        );
        let initial = (
            TextAlignValue::Left,
            TextAlignLastValue::Auto,
            TextJustifyValue::Auto,
            DirectionValue::Ltr,
        );

        assert_eq!(values("parent"), inherited);
        assert_eq!(values("inherit"), inherited);
        assert_eq!(values("unset"), inherited);
        assert_eq!(values("revert"), inherited);
        assert_eq!(values("initial"), initial);
        assert_eq!(values("terminal"), initial);
        assert_eq!(values("invalid"), inherited);
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
            parse_local_dimension_declaration("InHeRiT"),
            Some(LocalCascadeDeclaration::Inherit)
        );
        assert_eq!(
            parse_local_dimension_declaration("INITIAL"),
            Some(LocalCascadeDeclaration::Reset)
        );
        assert_eq!(
            parse_local_dimension_declaration("UnSeT"),
            Some(LocalCascadeDeclaration::Reset)
        );
        assert_eq!(
            parse_local_dimension_declaration("ReVeRt"),
            Some(LocalCascadeDeclaration::Reset)
        );
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
        assert_eq!(parse_local_dimension_declaration("inherit 24px"), None);
        assert_eq!(parse_local_dimension_declaration("initial 24px"), None);
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
    fn stylesheet_cascade_resolves_explicit_dimension_inheritance() {
        let document = NativeDocument::parse(
            r#"<style>
            #parent { width:24px; height:18px; min-width:4px; max-width:40px; min-height:6px; max-height:30px; }
            #child { width:inherit; height:INHERIT; min-width:InHeRiT; max-width:inherit; min-height:inherit; max-height:inherit; }
            #none-child { width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; }
            @layer base { #rollback-child { width:12px; height:13px; min-width:2px; max-width:20px; min-height:3px; max-height:21px; } }
            @layer theme { #rollback-child { width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; } #rollback-child { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } }
            #important-child { width:4px !important; height:5px !important; min-width:1px !important; max-width:6px !important; min-height:2px !important; max-height:7px !important; }
            #important-child { width:inherit !important; height:inherit !important; min-width:inherit !important; max-width:inherit !important; min-height:inherit !important; max-height:inherit !important; }
            #root-child { width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; }
            </style>
            <div id='parent'><div id='child'>Child</div></div>
            <div id='none-parent'><div id='none-child'>None</div></div>
            <div id='rollback-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='rollback-child'>Rollback</div></div>
            <div id='important-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='important-child'>Important</div></div>
            <div id='root-child'>Root</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

        let style = |id: &str| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
        };
        let dimensions = |style: NativeComputedStyle| {
            [
                style.width(),
                style.height(),
                style.min_width(),
                style.max_width(),
                style.min_height(),
                style.max_height(),
            ]
        };

        assert_eq!(
            dimensions(style("child")),
            [Some(24), Some(18), Some(4), Some(40), Some(6), Some(30)]
        );
        assert_eq!(
            dimensions(style("none-child")),
            [None, None, None, None, None, None]
        );
        assert_eq!(
            dimensions(style("rollback-child")),
            [Some(12), Some(13), Some(2), Some(20), Some(3), Some(21)]
        );
        assert_eq!(
            dimensions(style("important-child")),
            [Some(24), Some(18), Some(4), Some(40), Some(6), Some(30)]
        );
        assert_eq!(
            dimensions(style("root-child")),
            [None, None, None, None, None, None]
        );
    }

    #[test]
    fn stylesheet_cascade_resolves_dimension_css_wide_resets() {
        let document = NativeDocument::parse(
            r#"<style>
            #child-reset { width:INITIAL; height:unset; min-width:ReVeRt; max-width:initial; min-height:unset; max-height:revert; }
            #invalid-reset { width:initial; }
            #invalid-reset { width:bad; }
            #important-reset { width:4px !important; height:5px !important; min-width:1px !important; max-width:6px !important; min-height:2px !important; max-height:7px !important; }
            #important-reset { width:initial !important; height:unset !important; min-width:revert !important; max-width:initial !important; min-height:unset !important; max-height:revert !important; }
            @layer base { #rollback-child { width:12px; height:13px; min-width:2px; max-width:20px; min-height:3px; max-height:21px; } }
            @layer theme { #rollback-child { width:initial; height:unset; min-width:revert; max-width:initial; min-height:unset; max-height:revert; } #rollback-child { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } }
            #root-reset { width:initial; height:unset; min-width:revert; max-width:initial; min-height:unset; max-height:revert; }
            #revert-child { width:revert; height:revert; min-width:revert; max-width:revert; min-height:revert; max-height:revert; }
            </style>
            <div id='parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'>
                <div id='child-reset'>Reset</div>
                <div id='invalid-reset'>Invalid</div>
                <div id='important-reset'>Important</div>
            </div>
            <div id='rollback-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='rollback-child'>Rollback</div></div>
            <div id='revert-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='revert-child'>Revert</div></div>
            <div id='root-reset'>Root</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

        let style = |id: &str| {
            document
                .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
        };
        let dimensions = |style: NativeComputedStyle| {
            [
                style.width(),
                style.height(),
                style.min_width(),
                style.max_width(),
                style.min_height(),
                style.max_height(),
            ]
        };
        let none = [None, None, None, None, None, None];

        assert_eq!(dimensions(style("child-reset")), none);
        assert_eq!(dimensions(style("invalid-reset")), none);
        assert_eq!(dimensions(style("important-reset")), none);
        assert_eq!(
            dimensions(style("rollback-child")),
            [Some(12), Some(13), Some(2), Some(20), Some(3), Some(21)]
        );
        assert_eq!(dimensions(style("revert-child")), none);
        assert_eq!(dimensions(style("root-reset")), none);
    }

    #[test]
    fn local_box_model_declaration_parsers_accept_bounded_reset_and_revert_layer_forms() {
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
        assert_eq!(
            parse_local_box_edges("InItIaL"),
            Some([LocalCascadeDeclaration::Value(0); 4])
        );
        assert_eq!(
            parse_local_box_edge_pair("UNSET"),
            Some([LocalCascadeDeclaration::Value(0); 2])
        );
        assert_eq!(parse_local_box_edge_pair("revert 1px"), None);
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
        assert_eq!(
            parse_local_margin_declaration("ReVeRt"),
            Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)))
        );
        assert_eq!(
            parse_local_margin_edge_pair("initial"),
            Some([LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)); 2])
        );
        assert_eq!(parse_local_margin_edge_pair("unset auto"), None);
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
            parse_local_box_sizing_declaration("UNSET"),
            Some(LocalCascadeDeclaration::Value(NativeBoxSizing::ContentBox))
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

    #[test]
    fn logical_box_model_edges_parse_and_project_with_priority() {
        assert_eq!(
            parse_local_box_edge_pair("2px 3px"),
            Some([
                LocalCascadeDeclaration::Value(2),
                LocalCascadeDeclaration::Value(3),
            ])
        );
        assert_eq!(
            parse_local_box_edge_pair("ReVeRt-LaYeR"),
            Some([LocalCascadeDeclaration::RevertLayer; 2])
        );
        assert_eq!(parse_local_box_edge_pair("1px 2px 3px"), None);
        assert_eq!(parse_local_box_edge_pair("revert-layer 1px"), None);
        assert_eq!(
            parse_local_margin_edge_pair("auto 4px"),
            Some([
                LocalCascadeDeclaration::Value(NativeMarginValue::Auto),
                LocalCascadeDeclaration::Value(NativeMarginValue::Length(4)),
            ])
        );

        let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; padding-block:1px 2px; padding-inline:3px 4px; margin-block:auto 5px; margin-inline:6px auto; } #rtl { direction:rtl; padding-block:1px 2px; padding-inline:3px 4px; margin-block:auto 5px; margin-inline:6px auto; } } @layer theme { #ltr { padding-block-start:7px !important; margin-inline-start:9px !important; } } #ltr { padding-top:8px; padding-inline-start:10px; }".into(),
        ])
        .unwrap();
        let ltr = node("<div id='ltr'>Ltr</div>");
        let rtl = node("<div id='rtl'>Rtl</div>");
        let ltr_style = stylesheet.computed_for(&ltr);
        assert_eq!(
            ltr_style.padding(),
            NativeBoxEdges {
                top: 7,
                right: 4,
                bottom: 2,
                left: 10,
            }
        );
        assert_eq!(
            ltr_style.margin(),
            NativeBoxEdges {
                top: 0,
                right: 0,
                bottom: 5,
                left: 9,
            }
        );
        assert!(ltr_style.margin_auto().top());
        assert!(ltr_style.margin_auto().right());
        assert!(!ltr_style.margin_auto().left());

        let rtl_style = stylesheet.computed_for(&rtl);
        assert_eq!(
            rtl_style.padding(),
            NativeBoxEdges {
                top: 1,
                right: 3,
                bottom: 2,
                left: 4,
            }
        );
        assert_eq!(
            rtl_style.margin(),
            NativeBoxEdges {
                top: 0,
                right: 6,
                bottom: 5,
                left: 0,
            }
        );
        assert!(rtl_style.margin_auto().top());
        assert!(rtl_style.margin_auto().left());

        let inline = node(
            "<div style='direction:rtl;padding-top:1px;padding-block-start:2px;margin-left:3px;margin-inline-start:4px'>Inline</div>",
        );
        let inline_style = NativeStylesheet::default().computed_for(&inline);
        assert_eq!(inline_style.padding().top(), 2);
        assert_eq!(inline_style.margin().right(), 4);
    }
}
