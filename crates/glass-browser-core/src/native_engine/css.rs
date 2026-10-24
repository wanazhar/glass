use super::config::{MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_VIEWPORT_DIMENSION, Viewport};
use super::diagnostics::{NativeDiagnosticCode, NativeDiagnosticSink, NativeDiagnosticSource};
use super::dom::{
    HTML_NAMESPACE_URI, MATHML_NAMESPACE_URI, NativeDocument, NativeNode, NativeNodeId,
    SVG_NAMESPACE_URI, XLINK_NAMESPACE_URI, XML_NAMESPACE_URI, XMLNS_NAMESPACE_URI,
};
use super::error::NativeEngineError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

mod background_values;
pub(crate) use background_values::collect_background_image_sources;
use background_values::*;
pub(super) use background_values::{absolutize_stylesheet_urls, static_css_imports};

#[path = "css/border_parser.rs"]
mod border_parser;
use border_parser::*;

// Bound selector matching work as well as the retained stylesheet AST. With
// the native document's 4,096-node ceiling this limits a full selector pass
// to 16,777,216 rule/node pairs. The content-process protocol already uses
// 4,096 for this typed resource limit; keep parser and worker budgets aligned.
pub(crate) const MAX_NATIVE_STYLE_RULES: usize = 4_096;
// Public sites often declare more faces than the bounded runtime can load.
// Retain enough CSS rules for normal cascade matching while keeping actual
// font resource admission separately capped by MAX_NATIVE_FONT_FACES.
pub(crate) const MAX_NATIVE_FONT_FACE_RULES: usize = 64;
pub(crate) const MAX_NATIVE_FONT_PALETTE_RULES: usize = 16;
pub(crate) const MAX_NATIVE_FONT_PALETTE_OVERRIDES: usize = 32;
pub(crate) const MAX_NATIVE_GRID_TRACKS: usize = 8;
pub(crate) const MIN_NATIVE_FLEX_ITEM_ORDER: i32 = -1024;
pub(crate) const MAX_NATIVE_FLEX_ITEM_ORDER: i32 = 1024;
pub(crate) const MIN_NATIVE_Z_INDEX: i32 = -1_000_000;
pub(crate) const MAX_NATIVE_Z_INDEX: i32 = 1_000_000;
pub(crate) const MAX_NATIVE_FLEX_GROW: u32 = 1024;
pub(crate) const MAX_NATIVE_FLEX_SHRINK: u32 = 1024;
pub(crate) const MAX_NATIVE_TEXT_DECORATION_THICKNESS: u32 = 4;
pub(crate) const MIN_NATIVE_TEXT_UNDERLINE_OFFSET: i32 = -4;
pub(crate) const MAX_NATIVE_TEXT_UNDERLINE_OFFSET: i32 = 4;
pub(crate) const NATIVE_BACKGROUND_PERCENT_SCALE: i32 = 1_000;
const MAX_NATIVE_BACKGROUND_PERCENT: i32 = 100_000;
const MAX_SELECTOR_BYTES: usize = 256;
const MAX_SELECTOR_PARTS: usize = 8;
const MAX_NATIVE_CUSTOM_PROPERTIES_PER_RULE: usize = 16;
const MAX_NATIVE_CUSTOM_PROPERTY_NAME_BYTES: usize = 64;
const MAX_NATIVE_CUSTOM_PROPERTY_VALUE_BYTES: usize = 512;
const MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH: usize = 8;
const MAX_NATIVE_FONT_SIZE_VARIABLE_EXPRESSION_BYTES: usize = 256;
const MAX_NATIVE_FONT_FACE_SOURCE_BYTES: usize = 512 * 1024;
const MAX_NATIVE_FONT_FACE_SOURCES: usize = 8;
pub(crate) const MAX_NATIVE_FONT_FACE_UNICODE_RANGES: usize = 32;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeColor),
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
            Self::CustomProperty(_) | Self::CustomPropertyFallback(_, _) => match inherited {
                Some(color) => color,
                None => NativeColor::BLACK,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeColor),
    CustomPropertyShorthand(u64, u8),
    CustomPropertyShorthandFallback(u64, u8, NativeColor),
    CustomPropertyPair(u64, u8),
    CustomPropertyPairFallback(u64, u8, NativeColor),
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
            Self::CustomProperty(_)
            | Self::CustomPropertyFallback(_, _)
            | Self::CustomPropertyShorthand(_, _)
            | Self::CustomPropertyShorthandFallback(_, _, _)
            | Self::CustomPropertyPair(_, _)
            | Self::CustomPropertyPairFallback(_, _, _) => current_color,
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
enum NativeBackgroundColorShorthandFallback {
    Color(NativeColor),
    CurrentColor,
}

impl NativeBackgroundColorShorthandFallback {
    const fn resolve(self, current_color: NativeColor) -> NativeColor {
        match self {
            Self::Color(color) => color,
            Self::CurrentColor => current_color,
        }
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeColor),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, NativeBackgroundColorShorthandFallback),
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
            Self::CustomProperty(_)
            | Self::CustomPropertyFallback(_, _)
            | Self::CustomPropertyShorthand(_)
            | Self::CustomPropertyShorthandFallback(_, _) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBackgroundImageValue {
    Url(u32),
    None,
    Inherit,
    Unset,
    Initial,
    Revert,
    CustomProperty(u64),
    CustomPropertyFallback(u64, Option<u32>),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, Option<u32>),
}

impl NativeBackgroundImageValue {
    const fn resolve(self) -> Option<u32> {
        match self {
            Self::Url(source) => Some(source),
            Self::None
            | Self::Inherit
            | Self::Unset
            | Self::Initial
            | Self::Revert
            | Self::CustomProperty(_)
            | Self::CustomPropertyFallback(_, _)
            | Self::CustomPropertyShorthand(_)
            | Self::CustomPropertyShorthandFallback(_, _) => None,
        }
    }
}

/// The single-layer background repeat modes understood by the bounded native
/// display list. Two-value `repeat` syntax is normalized while parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeBackgroundRepeatFallback {
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeBackgroundRepeat {
    #[default]
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBackgroundRepeatFallback),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, NativeBackgroundRepeatFallback),
}

/// A background position component. Percentages use thousandths of the
/// available position range (`1000` means `100%`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeBackgroundPositionComponent {
    Length(i32),
    Percentage(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeBackgroundPosition {
    pub(crate) x: NativeBackgroundPositionComponent,
    pub(crate) y: NativeBackgroundPositionComponent,
}

impl Default for NativeBackgroundPosition {
    fn default() -> Self {
        Self {
            x: NativeBackgroundPositionComponent::Percentage(0),
            y: NativeBackgroundPositionComponent::Percentage(0),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBackgroundPositionDeclaration {
    Value(NativeBackgroundPosition),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBackgroundPosition),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, NativeBackgroundPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeBackgroundSizeComponent {
    Auto,
    Length(u32),
    Percentage(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeBackgroundSize {
    Explicit {
        width: NativeBackgroundSizeComponent,
        height: NativeBackgroundSizeComponent,
    },
    Cover,
    Contain,
}

impl Default for NativeBackgroundSize {
    fn default() -> Self {
        Self::Explicit {
            width: NativeBackgroundSizeComponent::Auto,
            height: NativeBackgroundSizeComponent::Auto,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBackgroundSizeDeclaration {
    Value(NativeBackgroundSize),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBackgroundSize),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, NativeBackgroundSize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBackgroundShorthandDeclaration {
    Value(NativeBackgroundShorthand),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBackgroundShorthand),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeBackgroundShorthand {
    color: NativeBackgroundColorValue,
    image: NativeBackgroundImageValue,
    repeat: NativeBackgroundRepeat,
    position: NativeBackgroundPosition,
    size: NativeBackgroundSize,
}

/// Bounded text-decoration patterns owned separately from border styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeTextDecorationStyle),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationThicknessDeclaration {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextUnderlineOffsetDeclaration {
    Value(i32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, i32),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeColor),
}

/// Bounded inherited glyph-intersection behavior for text decorations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NativeTextDecorationSkipInk {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeTextDecorationSkipInkDeclaration {
    Value(NativeTextDecorationSkipInk),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeTextDecorationSkipInk),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

/// Bounded inherited fixed-cell whitespace behavior for text decorations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeTextDecorationSkipSpaces),
    Inherit,
    Unset,
    Revert,
    RevertLayer,
}

/// Bounded physical circular radii for the top-left, top-right, bottom-right,
/// and bottom-left corners of one native box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBorderRadius),
    CustomPropertyCorner(u64),
    CustomPropertyCornerFallback(u64, u32),
    Inherit,
    Unset,
    Initial,
    Revert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeBorderSide {
    width: u32,
    style: NativeBorderStyle,
    color: NativeColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeBorderDeclaration {
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBorderSide),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeBoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DisplayValue {
    #[default]
    Auto,
    None,
    Block,
    Inline,
    Contents,
    Flex,
    Grid,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativePositionValue {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeZIndexValue {
    #[default]
    Auto,
    Integer(i32),
}

impl NativeZIndexValue {
    pub(crate) const fn is_auto(self) -> bool {
        matches!(self, Self::Auto)
    }

    pub(crate) const fn value(self) -> i32 {
        match self {
            Self::Auto => 0,
            Self::Integer(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativePointerEventsValue {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerEventsDeclarationValue {
    Value(NativePointerEventsValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativePointerEventsValue),
}

impl NativePointerEventsValue {
    pub(crate) const fn allows_hit_testing(self) -> bool {
        matches!(self, Self::Auto)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativePositionOffset {
    #[default]
    Auto,
    Length(i32),
}

impl NativePositionOffset {
    pub(crate) const fn length(self) -> Option<i32> {
        match self {
            Self::Auto => None,
            Self::Length(value) => Some(value),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeGridTrack {
    Length(u32),
    Fr(u32),
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeGridTrackList {
    tracks: [NativeGridTrack; MAX_NATIVE_GRID_TRACKS],
    len: u8,
}

impl NativeGridTrackList {
    pub(crate) const fn len(self) -> usize {
        self.len as usize
    }

    pub(crate) const fn track(self, index: usize) -> NativeGridTrack {
        self.tracks[index]
    }

    fn push(&mut self, track: NativeGridTrack) -> bool {
        let index = usize::from(self.len);
        if index >= MAX_NATIVE_GRID_TRACKS {
            return false;
        }
        self.tracks[index] = track;
        self.len = self.len.saturating_add(1);
        true
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GridTrackListDeclaration {
    Value(NativeGridTrackList),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeGridTrackList),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WhiteSpaceValue {
    #[default]
    Normal,
    PreLine,
    Pre,
    PreWrap,
    NoWrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhiteSpaceDeclarationValue {
    Value(WhiteSpaceValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, WhiteSpaceValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineHeightDeclarationValue {
    Value(u32),
    Initial,
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
enum TextAlignDeclarationValue {
    Value(TextAlignValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, TextAlignValue),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
enum TextAlignLastDeclarationValue {
    Value(TextAlignLastValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, TextAlignLastValue),
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TextJustifyValue {
    #[default]
    Auto,
    None,
    InterWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextJustifyDeclarationValue {
    Value(TextJustifyValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, TextJustifyValue),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, JustifyContentValue),
    CustomPropertyPlaceContent(u64),
    CustomPropertyPlaceContentFallback(u64, JustifyContentValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, AlignItemsValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, AlignSelfValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, AlignContentValue),
    CustomPropertyPlaceContent(u64),
    CustomPropertyPlaceContentFallback(u64, AlignContentValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, FlexDirectionValue),
    CustomPropertyFlow(u64),
    CustomPropertyFlowFallback(u64, FlexDirectionValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DirectionValue {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectionDeclarationValue {
    Value(DirectionValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, DirectionValue),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FlexWrapValue {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexWrapDeclaration {
    Value(FlexWrapValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, FlexWrapValue),
    CustomPropertyFlow(u64),
    CustomPropertyFlowFallback(u64, FlexWrapValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FlexBasisValue {
    #[default]
    Auto,
    Length(u32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeOrderValue(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexItemOrderDeclaration {
    Value(NativeOrderValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeOrderValue),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexGrowDeclaration {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, u32),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexShrinkDeclaration {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, u32),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlexBasisDeclaration {
    Value(FlexBasisValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, FlexBasisValue),
    CustomPropertyShorthand(u64),
    CustomPropertyShorthandFallback(u64, FlexBasisValue),
    Inherit,
    Reset,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    CustomProperty(u64),
    CustomPropertyFallback(u64, TextDecorationValue),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TextTransformValue {
    #[default]
    None,
    Uppercase,
    Lowercase,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextTransformDeclarationValue {
    Value(TextTransformValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, TextTransformValue),
}

/// The bounded OpenType ligature controls exposed by
/// `font-variant-ligatures`. The initial value enables common and contextual
/// ligatures, while discretionary and historical ligatures remain opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontVariantLigatures {
    pub(crate) common: bool,
    pub(crate) discretionary: bool,
    pub(crate) historical: bool,
    pub(crate) contextual: bool,
}

impl Default for NativeFontVariantLigatures {
    fn default() -> Self {
        Self {
            common: true,
            discretionary: false,
            historical: false,
            contextual: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariantLigaturesDeclarationValue {
    Value(NativeFontVariantLigatures),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariantLigatures),
}

/// The bounded OpenType capitalization controls exposed by
/// `font-variant-caps`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariantCapsDeclarationValue {
    Value(NativeFontVariantCaps),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariantCaps),
}

/// The bounded OpenType subscript/superscript controls exposed by
/// `font-variant-position`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantPosition {
    #[default]
    Normal,
    Sub,
    Super,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariantPositionDeclarationValue {
    Value(NativeFontVariantPosition),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariantPosition),
}

/// The bounded OpenType alternate control exposed by
/// `font-variant-alternates`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantAlternates {
    #[default]
    Normal,
    HistoricalForms,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariantAlternatesDeclarationValue {
    Value(NativeFontVariantAlternates),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariantAlternates),
}

/// The bounded OpenType language-system override exposed by
/// `font-language-override`. CSS supplies this as a four-byte string tag;
/// `None` represents the initial `normal` value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontLanguageOverride {
    pub(crate) tag: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontLanguageOverrideDeclarationValue {
    Value(NativeFontLanguageOverride),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontLanguageOverride),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantEastAsianForm {
    #[default]
    Normal,
    Jis78,
    Jis83,
    Jis90,
    Jis04,
    Simplified,
    Traditional,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantEastAsianWidth {
    #[default]
    Normal,
    Full,
    Proportional,
}

/// The bounded OpenType controls exposed by `font-variant-east-asian`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontVariantEastAsian {
    pub(crate) form: NativeFontVariantEastAsianForm,
    pub(crate) width: NativeFontVariantEastAsianWidth,
    pub(crate) ruby: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariantEastAsianDeclarationValue {
    Value(NativeFontVariantEastAsian),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariantEastAsian),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantNumericFigure {
    #[default]
    Normal,
    Lining,
    Oldstyle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantNumericSpacing {
    #[default]
    Normal,
    Proportional,
    Tabular,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontVariantNumericFraction {
    #[default]
    Normal,
    Diagonal,
    Stacked,
}

/// The bounded OpenType numeric controls exposed by `font-variant-numeric`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontVariantNumeric {
    pub(crate) figure: NativeFontVariantNumericFigure,
    pub(crate) spacing: NativeFontVariantNumericSpacing,
    pub(crate) fraction: NativeFontVariantNumericFraction,
    pub(crate) ordinal: bool,
    pub(crate) slashed_zero: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariantNumericDeclarationValue {
    Value(NativeFontVariantNumeric),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariantNumeric),
}

const MAX_NATIVE_FONT_FEATURES: usize = 16;
const MAX_NATIVE_FONT_VARIATIONS: usize = 8;
const MAX_NATIVE_FONT_VARIATION_VALUE_MILLI: i32 = 32_768_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontFeature {
    pub(crate) tag: [u8; 4],
    pub(crate) value: u32,
}

impl Default for NativeFontFeature {
    fn default() -> Self {
        Self {
            tag: [b' '; 4],
            value: 0,
        }
    }
}

/// The bounded low-level OpenType feature list exposed by
/// `font-feature-settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontFeatureSettings {
    pub(crate) values: [NativeFontFeature; MAX_NATIVE_FONT_FEATURES],
    pub(crate) count: u8,
}

impl Default for NativeFontFeatureSettings {
    fn default() -> Self {
        Self {
            values: [NativeFontFeature::default(); MAX_NATIVE_FONT_FEATURES],
            count: 0,
        }
    }
}

impl NativeFontFeatureSettings {
    pub(crate) fn values(&self) -> &[NativeFontFeature] {
        &self.values[..usize::from(self.count).min(MAX_NATIVE_FONT_FEATURES)]
    }

    pub(crate) fn contains_tag(&self, tag: &[u8; 4]) -> bool {
        self.values().iter().any(|feature| &feature.tag == tag)
    }

    pub(crate) fn with_overrides(self, overrides: Self) -> Self {
        let mut merged = Self::default();
        for feature in overrides.values().iter().chain(self.values()) {
            if !merged.contains_tag(&feature.tag) && !merged.insert(*feature) {
                break;
            }
        }
        merged
    }

    fn insert(&mut self, feature: NativeFontFeature) -> bool {
        let count = usize::from(self.count);
        if let Some(existing) = self
            .values
            .get_mut(..count)
            .and_then(|values| values.iter_mut().find(|item| item.tag == feature.tag))
        {
            *existing = feature;
            true
        } else if let Some(slot) = self.values.get_mut(count) {
            *slot = feature;
            self.count = self.count.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        usize::from(self.count) <= MAX_NATIVE_FONT_FEATURES
            && self.values().iter().all(|feature| {
                feature.tag.iter().all(|byte| {
                    (0x20..=0x7e).contains(byte) && !matches!(*byte, b'\'' | b'"' | b',')
                })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontFeatureSettingsDeclarationValue {
    Value(NativeFontFeatureSettings),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontFeatureSettings),
}

/// One bounded OpenType variation axis value expressed in thousandths so the
/// native style and content-process wire remain deterministic and float-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontVariation {
    pub(crate) tag: [u8; 4],
    pub(crate) value_milli: i32,
}

impl Default for NativeFontVariation {
    fn default() -> Self {
        Self {
            tag: [b' '; 4],
            value_milli: 0,
        }
    }
}

/// The bounded inherited OpenType variation list exposed by
/// `font-variation-settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontVariationSettings {
    pub(crate) values: [NativeFontVariation; MAX_NATIVE_FONT_VARIATIONS],
    pub(crate) count: u8,
}

impl Default for NativeFontVariationSettings {
    fn default() -> Self {
        Self {
            values: [NativeFontVariation::default(); MAX_NATIVE_FONT_VARIATIONS],
            count: 0,
        }
    }
}

impl NativeFontVariationSettings {
    pub(crate) fn values(&self) -> &[NativeFontVariation] {
        &self.values[..usize::from(self.count).min(MAX_NATIVE_FONT_VARIATIONS)]
    }

    /// Overlay authored element coordinates on a face's descriptor defaults.
    /// Authored coordinates are inserted first so the fixed bound never drops
    /// an explicit element value when the two lists use different axes.
    pub(crate) fn with_overrides(self, overrides: Self) -> Self {
        let mut merged = Self::default();
        for variation in overrides.values().iter().chain(self.values()) {
            let count = usize::from(merged.count);
            if merged
                .values
                .get(..count)
                .is_some_and(|values| values.iter().any(|item| item.tag == variation.tag))
            {
                continue;
            } else if count < MAX_NATIVE_FONT_VARIATIONS {
                merged.values[count] = *variation;
                merged.count = merged.count.saturating_add(1);
            }
        }
        merged
    }

    pub(crate) fn is_valid(self) -> bool {
        usize::from(self.count) <= MAX_NATIVE_FONT_VARIATIONS
            && self.values().iter().all(|variation| {
                variation.tag.iter().all(|byte| {
                    (0x20..=0x7e).contains(byte) && !matches!(*byte, b'\'' | b'"' | b'\\' | b',')
                }) && i64::from(variation.value_milli).unsigned_abs()
                    <= i64::from(MAX_NATIVE_FONT_VARIATION_VALUE_MILLI) as u64
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontVariationSettingsDeclarationValue {
    Value(NativeFontVariationSettings),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontVariationSettings),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontKerning {
    #[default]
    Auto,
    Normal,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontKerningDeclarationValue {
    Value(NativeFontKerning),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontKerning),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontOpticalSizing {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontOpticalSizingDeclarationValue {
    Value(NativeFontOpticalSizing),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontOpticalSizing),
}

const MAX_NATIVE_FONT_PALETTE_NAME_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontPaletteName {
    bytes: [u8; MAX_NATIVE_FONT_PALETTE_NAME_BYTES],
    len: u8,
}

impl NativeFontPaletteName {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        if bytes.len() > MAX_NATIVE_FONT_PALETTE_NAME_BYTES
            || !bytes.starts_with(b"--")
            || bytes.len() < 3
            || bytes[2..].iter().any(|byte| !is_identifier_char(*byte))
        {
            return None;
        }
        let mut name = Self::default();
        name.bytes[..bytes.len()].copy_from_slice(bytes);
        name.len = u8::try_from(bytes.len()).ok()?;
        Some(name)
    }
}

impl Default for NativeFontPaletteName {
    fn default() -> Self {
        Self {
            bytes: [0; MAX_NATIVE_FONT_PALETTE_NAME_BYTES],
            len: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontPalette {
    #[default]
    Normal,
    Light,
    Dark,
    Named(NativeFontPaletteName),
    Base(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontPaletteDeclarationValue {
    Value(NativeFontPalette),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontPalette),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeFontPaletteOverride {
    pub(crate) palette_index: u16,
    pub(crate) color: NativeColor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FontWeightValue {
    #[default]
    Normal,
    Bold,
    Numeric(u16),
}

impl FontWeightValue {
    pub(crate) const fn numeric(self) -> u16 {
        match self {
            Self::Normal => 400,
            Self::Bold => 700,
            Self::Numeric(value) => value,
        }
    }

    pub(crate) const fn from_numeric(value: u16) -> Option<Self> {
        if value == 0 || value > 1000 {
            return None;
        }
        Some(match value {
            400 => Self::Normal,
            700 => Self::Bold,
            value => Self::Numeric(value),
        })
    }

    pub(crate) const fn is_bold(self) -> bool {
        self.numeric() >= 600
    }
}

/// A bounded absolute `font-weight` descriptor range. A singleton retains the
/// existing CSS keyword/number behavior while a true range admits a face for
/// every requested weight inside its interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontWeightRange {
    pub(crate) min: u16,
    pub(crate) max: u16,
}

impl Default for NativeFontWeightRange {
    fn default() -> Self {
        Self { min: 400, max: 400 }
    }
}

impl NativeFontWeightRange {
    pub(crate) const fn singleton(weight: FontWeightValue) -> Self {
        let weight = weight.numeric();
        Self {
            min: weight,
            max: weight,
        }
    }

    pub(crate) const fn contains(self, weight: u16) -> bool {
        weight >= self.min && weight <= self.max
    }

    pub(crate) const fn distance(self, weight: u16) -> u16 {
        if self.contains(weight) {
            0
        } else if weight < self.min {
            self.min.saturating_sub(weight)
        } else {
            weight.saturating_sub(self.max)
        }
    }

    /// A range's nominal request is used only for `local()` source lookup.
    pub(crate) const fn nominal(self) -> u16 {
        self.min.saturating_add(self.max).saturating_div(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontWeightDeclarationValue {
    Absolute(FontWeightValue),
    Lighter,
    Bolder,
    CustomProperty(u64),
    CustomPropertyFallback(u64, FontWeightValue),
}

pub(crate) fn format_font_weight(weight: FontWeightValue) -> String {
    weight.numeric().to_string()
}

pub(crate) fn format_font_weight_range(range: NativeFontWeightRange) -> String {
    if range.min == range.max {
        format_font_weight(
            FontWeightValue::from_numeric(range.min).unwrap_or(FontWeightValue::Normal),
        )
    } else {
        format!("{} {}", range.min, range.max)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FontStyleValue {
    #[default]
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontStyleDeclarationValue {
    Normal,
    Italic,
    CustomProperty(u64),
    CustomPropertyFallback(u64, FontStyleValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeGenericFontFamily {
    SansSerif,
    Serif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum NativeFontFamilyValue {
    /// Preserve the legacy fixed-cell path when no native font declaration
    /// has opted the element into real-font metrics yet.
    #[default]
    Fallback,
    Generic(NativeGenericFontFamily),
    Named(u64),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct NativeFontFamilyList {
    values: [NativeFontFamilyValue; 4],
    len: u8,
}

impl PartialEq for NativeFontFamilyList {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && self
                .values
                .iter()
                .take(usize::from(self.len))
                .eq(other.values.iter().take(usize::from(other.len)))
    }
}

impl Eq for NativeFontFamilyList {}

impl Default for NativeFontFamilyList {
    fn default() -> Self {
        Self::single(NativeFontFamilyValue::Fallback)
    }
}

impl NativeFontFamilyList {
    pub(crate) const fn single(value: NativeFontFamilyValue) -> Self {
        Self {
            values: [value; 4],
            len: 1,
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut families = [NativeFontFamilyValue::Fallback; 4];
        let mut len = 0usize;
        for raw_family in value.split(',') {
            let family = raw_family.trim();
            if family.is_empty() || len == families.len() {
                return None;
            }
            let unquoted = if (family.starts_with('"') && family.ends_with('"'))
                || (family.starts_with('\'') && family.ends_with('\''))
            {
                let family = &family[1..family.len().saturating_sub(1)];
                if family.is_empty() || family.contains(['"', '\'']) {
                    return None;
                }
                family
            } else if family.contains(['"', '\'']) {
                return None;
            } else {
                family
            };
            families[len] = parse_font_family_name(unquoted)?;
            len += 1;
        }
        (len > 0).then_some(Self {
            values: families,
            len: u8::try_from(len).ok()?,
        })
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = NativeFontFamilyValue> {
        let len = usize::from(self.len);
        self.values.into_iter().take(len)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontFamilyDeclarationValue {
    Value(NativeFontFamilyList),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontFamilyList),
}

/// A bounded `@font-face` descriptor set. Keeping ordered source candidates
/// here lets the resource owner resolve and admit them without giving CSS
/// parsing access to filesystem or network capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeFontFaceSource {
    Local(String),
    Url(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeUnicodeRange {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

impl NativeUnicodeRange {
    pub(crate) fn contains(self, character: char) -> bool {
        let code_point = u32::from(character);
        (self.start..=self.end).contains(&code_point)
    }
}

/// A bounded `font-stretch` descriptor range stored as percentage tenths.
/// `1000` represents `100%`, preserving the named CSS values without floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontStretchRange {
    pub(crate) min: u16,
    pub(crate) max: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontStretchDeclarationValue {
    Value(NativeFontStretchRange),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontStretchRange),
}

impl Default for NativeFontStretchRange {
    fn default() -> Self {
        Self {
            min: 1000,
            max: 1000,
        }
    }
}

impl NativeFontStretchRange {
    pub(crate) fn contains(self, stretch: u16) -> bool {
        (self.min..=self.max).contains(&stretch)
    }

    pub(crate) fn distance(self, stretch: u16) -> u16 {
        if self.contains(stretch) {
            0
        } else if stretch < self.min {
            self.min.saturating_sub(stretch)
        } else {
            stretch.saturating_sub(self.max)
        }
    }

    /// A singleton descriptor identifies the source face's nominal width.
    /// Ranges represent variable-capable spans and use normal width as the
    /// synthetic-scaling baseline until variation-axis selection is wired.
    pub(crate) fn nominal(self) -> u16 {
        if self.min == self.max { self.min } else { 1000 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NativeFontDisplay {
    #[default]
    Auto,
    Block,
    Swap,
    Fallback,
    Optional,
}

impl NativeFontDisplay {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "block" => Some(Self::Block),
            "swap" => Some(Self::Swap),
            "fallback" => Some(Self::Fallback),
            "optional" => Some(Self::Optional),
            _ => None,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Block => "block",
            Self::Swap => "swap",
            Self::Fallback => "fallback",
            Self::Optional => "optional",
        }
    }
}

pub(crate) const DEFAULT_NATIVE_FONT_SIZE_ADJUST: u16 = 1000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeFontMetricOverrides {
    pub(crate) ascent: Option<u16>,
    pub(crate) descent: Option<u16>,
    pub(crate) line_gap: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFontFaceRule {
    pub(crate) family: String,
    pub(crate) family_key: u64,
    pub(crate) sources: Vec<NativeFontFaceSource>,
    pub(crate) weight: NativeFontWeightRange,
    pub(crate) style: FontStyleValue,
    pub(crate) stretch: NativeFontStretchRange,
    pub(crate) unicode_ranges: Vec<NativeUnicodeRange>,
    pub(crate) variation_settings: NativeFontVariationSettings,
    pub(crate) feature_settings: NativeFontFeatureSettings,
    pub(crate) size_adjust: u16,
    pub(crate) metric_overrides: NativeFontMetricOverrides,
    pub(crate) font_display: NativeFontDisplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeFontPaletteValuesRule {
    pub(crate) name: NativeFontPaletteName,
    pub(crate) base_palette: u16,
    pub(crate) overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WordBreakValue {
    #[default]
    Normal,
    BreakAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordBreakDeclarationValue {
    Value(WordBreakValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, WordBreakValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordSpacingDeclarationValue {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LetterSpacingDeclarationValue {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
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
enum NativeViewportFontSizeUnit {
    Width,
    Height,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeFontSizeFallbackValue {
    Pixels(u32),
    Relative(u32),
    RootRelative(u32),
    Viewport(u32, NativeViewportFontSizeUnit),
    Calculation(NativeFontSizeCalculation),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeFontSizeVariableExpression {
    bytes: [u8; MAX_NATIVE_FONT_SIZE_VARIABLE_EXPRESSION_BYTES],
    len: u16,
}

impl NativeFontSizeVariableExpression {
    fn from_text(value: &str) -> Option<Self> {
        if value.is_empty()
            || value.len() > MAX_NATIVE_FONT_SIZE_VARIABLE_EXPRESSION_BYTES
            || !value.is_ascii()
        {
            return None;
        }
        let mut bytes = [0_u8; MAX_NATIVE_FONT_SIZE_VARIABLE_EXPRESSION_BYTES];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self {
            bytes,
            len: u16::try_from(value.len()).ok()?,
        })
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("font-size variable expression is ASCII")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeFontSizeDeclarationValue {
    Pixels(u32),
    Relative(u32),
    RootRelative(u32),
    Viewport(u32, NativeViewportFontSizeUnit),
    Calculation(NativeFontSizeCalculation),
    VariableCalculation(NativeFontSizeVariableExpression),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeFontSizeFallbackValue),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeFontSizeCalculation {
    absolute_milli: i64,
    parent_scale_milli: i64,
    root_scale_milli: i64,
    viewport_width_scale_milli: i64,
    viewport_height_scale_milli: i64,
    viewport_min_scale_milli: i64,
    viewport_max_scale_milli: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeFontVariantShorthand {
    ligatures: InheritedTextDeclaration<NativeFontVariantLigatures>,
    caps: InheritedTextDeclaration<NativeFontVariantCaps>,
    position: InheritedTextDeclaration<NativeFontVariantPosition>,
    alternates: InheritedTextDeclaration<NativeFontVariantAlternates>,
    east_asian: InheritedTextDeclaration<NativeFontVariantEastAsian>,
    numeric: InheritedTextDeclaration<NativeFontVariantNumeric>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FontVariantShorthandCustomPropertyDeclaration {
    ligatures: InheritedTextDeclaration<FontVariantLigaturesDeclarationValue>,
    caps: InheritedTextDeclaration<FontVariantCapsDeclarationValue>,
    position: InheritedTextDeclaration<FontVariantPositionDeclarationValue>,
    alternates: InheritedTextDeclaration<FontVariantAlternatesDeclarationValue>,
    east_asian: InheritedTextDeclaration<FontVariantEastAsianDeclarationValue>,
    numeric: InheritedTextDeclaration<FontVariantNumericDeclarationValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalCascadeDeclaration<T> {
    Value(T),
    Inherit,
    Reset,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TextOverflowValue {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextOverflowDeclarationValue {
    Value(TextOverflowValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, TextOverflowValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextIndentDeclarationValue {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum VerticalAlignValue {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerticalAlignDeclarationValue {
    Value(VerticalAlignValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, VerticalAlignValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeGapValue {
    row: u32,
    column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapShorthandDeclaration {
    Value(NativeGapValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeGapValue),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapComponentDeclaration {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
    CustomPropertyShorthand(u64, u8),
    CustomPropertyShorthandFallback(u64, u8, u32),
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub(crate) justify_content: JustifyContentValue,
    pub(crate) align_items: AlignItemsValue,
    pub(crate) align_self: AlignSelfValue,
    pub(crate) align_content: AlignContentValue,
    pub(crate) flex_direction: FlexDirectionValue,
    pub(crate) flex_wrap: FlexWrapValue,
    pub(crate) flex_item_order: NativeOrderValue,
    pub(crate) row_gap: u32,
    pub(crate) column_gap: u32,
    pub(crate) direction: DirectionValue,
    pub(crate) flex_grow: u32,
    pub(crate) flex_shrink: u32,
    pub(crate) flex_basis: FlexBasisValue,
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
    pub(crate) font_variant_ligatures: NativeFontVariantLigatures,
    pub(crate) font_variant_caps: NativeFontVariantCaps,
    pub(crate) font_variant_position: NativeFontVariantPosition,
    pub(crate) font_variant_alternates: NativeFontVariantAlternates,
    pub(crate) font_language_override: NativeFontLanguageOverride,
    pub(crate) font_variation_settings: NativeFontVariationSettings,
    pub(crate) font_variant_east_asian: NativeFontVariantEastAsian,
    pub(crate) font_variant_numeric: NativeFontVariantNumeric,
    pub(crate) font_feature_settings: NativeFontFeatureSettings,
    pub(crate) font_kerning: NativeFontKerning,
    pub(crate) font_optical_sizing: NativeFontOpticalSizing,
    pub(crate) font_palette: NativeFontPalette,
    pub(crate) font_weight: FontWeightValue,
    pub(crate) font_style: FontStyleValue,
    pub(crate) font_stretch: NativeFontStretchRange,
    pub(crate) font_family: NativeFontFamilyList,
    pub(crate) font_size: u32,
    pub(crate) root_font_size: u32,
    pub(crate) viewport: Viewport,
    pub(crate) word_break: WordBreakValue,
    pub(crate) text_overflow: TextOverflowValue,
    pub(crate) overflow_x: OverflowValue,
    pub(crate) overflow_y: OverflowValue,
    pub(crate) vertical_align: VerticalAlignValue,
    pub(crate) text_indent: u32,
    pub(crate) word_spacing: u32,
    pub(crate) letter_spacing: u32,
    pub(crate) pointer_events: NativePointerEventsValue,
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
            justify_content: JustifyContentValue::FlexStart,
            align_items: AlignItemsValue::FlexStart,
            align_self: AlignSelfValue::Auto,
            align_content: AlignContentValue::FlexStart,
            flex_direction: FlexDirectionValue::Row,
            flex_wrap: FlexWrapValue::NoWrap,
            flex_item_order: NativeOrderValue::default(),
            row_gap: 0,
            column_gap: 0,
            direction: DirectionValue::Ltr,
            flex_grow: 0,
            flex_shrink: 1,
            flex_basis: FlexBasisValue::Auto,
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
            font_variant_ligatures: NativeFontVariantLigatures::default(),
            font_variant_caps: NativeFontVariantCaps::Normal,
            font_variant_position: NativeFontVariantPosition::Normal,
            font_variant_alternates: NativeFontVariantAlternates::Normal,
            font_language_override: NativeFontLanguageOverride::default(),
            font_variation_settings: NativeFontVariationSettings::default(),
            font_variant_east_asian: NativeFontVariantEastAsian::default(),
            font_variant_numeric: NativeFontVariantNumeric::default(),
            font_feature_settings: NativeFontFeatureSettings::default(),
            font_kerning: NativeFontKerning::Auto,
            font_optical_sizing: NativeFontOpticalSizing::Auto,
            font_palette: NativeFontPalette::Normal,
            font_weight: FontWeightValue::Normal,
            font_style: FontStyleValue::Normal,
            font_stretch: NativeFontStretchRange::default(),
            font_family: NativeFontFamilyList::default(),
            font_size: super::font::DEFAULT_NATIVE_FONT_SIZE,
            root_font_size: super::font::DEFAULT_NATIVE_FONT_SIZE,
            viewport: Viewport::default(),
            word_break: WordBreakValue::Normal,
            text_overflow: TextOverflowValue::Clip,
            overflow_x: OverflowValue::Other,
            overflow_y: OverflowValue::Other,
            vertical_align: VerticalAlignValue::Baseline,
            text_indent: 0,
            word_spacing: 0,
            letter_spacing: 0,
            pointer_events: NativePointerEventsValue::Auto,
        }
    }
}

/// Bounded physical top, right, bottom, and left box values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeComputedStyle {
    display: DisplayValue,
    position: NativePositionValue,
    z_index: NativeZIndexValue,
    pointer_events: NativePointerEventsValue,
    top: NativePositionOffset,
    right: NativePositionOffset,
    bottom: NativePositionOffset,
    left: NativePositionOffset,
    grid_template_columns: NativeGridTrackList,
    grid_template_rows: NativeGridTrackList,
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
    #[serde(default)]
    font_variant_ligatures: NativeFontVariantLigatures,
    #[serde(default)]
    font_variant_caps: NativeFontVariantCaps,
    #[serde(default)]
    font_variant_position: NativeFontVariantPosition,
    #[serde(default)]
    font_variant_alternates: NativeFontVariantAlternates,
    #[serde(default)]
    font_language_override: NativeFontLanguageOverride,
    #[serde(default)]
    font_variation_settings: NativeFontVariationSettings,
    #[serde(default)]
    font_variant_east_asian: NativeFontVariantEastAsian,
    #[serde(default)]
    font_variant_numeric: NativeFontVariantNumeric,
    #[serde(default)]
    font_feature_settings: NativeFontFeatureSettings,
    #[serde(default)]
    font_kerning: NativeFontKerning,
    #[serde(default)]
    font_optical_sizing: NativeFontOpticalSizing,
    #[serde(default)]
    font_palette: NativeFontPalette,
    font_weight: FontWeightValue,
    font_style: FontStyleValue,
    font_stretch: NativeFontStretchRange,
    font_family: NativeFontFamilyList,
    font_size: u32,
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
    background_image: Option<u32>,
    background_repeat: NativeBackgroundRepeat,
    background_position: NativeBackgroundPosition,
    background_size: NativeBackgroundSize,
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
    overflow_x: OverflowValue,
    overflow_y: OverflowValue,
    overflow_clip_x: bool,
    overflow_clip_y: bool,
}

mod computed_style;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NativeStylesheet {
    rules: Vec<NativeStyleRule>,
    rule_index: NativeStyleRuleIndex,
    background_image_rule_index: NativeStyleRuleIndex,
    background_image_sources: BTreeMap<u32, String>,
    font_face_rules: Vec<NativeFontFaceRule>,
    font_palette_values: Vec<NativeFontPaletteValuesRule>,
}

mod stylesheet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityValue {
    Hidden,
    Other,
    CustomProperty(u64),
    CustomPropertyFallback(u64, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayDeclarationValue {
    Value(DisplayValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, DisplayValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositionDeclarationValue {
    Value(NativePositionValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativePositionValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZIndexDeclarationValue {
    Value(NativeZIndexValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeZIndexValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositionOffsetDeclarationValue {
    Value(NativePositionOffset),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativePositionOffset),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DimensionDeclarationValue {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoxSizingDeclarationValue {
    Value(NativeBoxSizing),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBoxSizing),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaddingDeclarationValue {
    Value(u32),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u32),
    CustomPropertyShorthand(u64, u8),
    CustomPropertyShorthandFallback(u64, u8, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarginDeclarationValue {
    Value(NativeMarginValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeMarginValue),
    CustomPropertyShorthand(u64, u8),
    CustomPropertyShorthandFallback(u64, u8, NativeMarginValue),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorderWidthDeclarationValue {
    Value(NativeBorderWidthValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBorderWidthValue),
    CustomPropertyShorthand(u64, u8),
    CustomPropertyShorthandFallback(u64, u8, NativeBorderWidthValue),
    CustomPropertyPair(u64, u8),
    CustomPropertyPairFallback(u64, u8, NativeBorderWidthValue),
}

impl BorderWidthDeclarationValue {
    const fn is_css_wide(self) -> bool {
        matches!(
            self,
            Self::Value(value) if value.is_css_wide()
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorderStyleDeclarationValue {
    Value(NativeBorderStyleValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, NativeBorderStyleValue),
    CustomPropertyShorthand(u64, u8),
    CustomPropertyShorthandFallback(u64, u8, NativeBorderStyleValue),
    CustomPropertyPair(u64, u8),
    CustomPropertyPairFallback(u64, u8, NativeBorderStyleValue),
}

impl BorderStyleDeclarationValue {
    const fn is_css_wide(self) -> bool {
        matches!(
            self,
            Self::Value(value) if value.is_css_wide()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpacityDeclarationValue {
    Value(u8),
    CustomProperty(u64),
    CustomPropertyFallback(u64, u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum OverflowValue {
    Hidden,
    Clip,
    Auto,
    Scroll,
    #[default]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverflowDeclarationValue {
    Value(OverflowValue),
    CustomProperty(u64),
    CustomPropertyFallback(u64, OverflowValue),
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

type NativeLocalCascadeCandidates<T> =
    [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
type NativeTextCascadeCandidates<T> =
    [Option<CascadeValue<InheritedTextDeclaration<T>>>; MAX_NATIVE_TEXT_CASCADE_LAYERS];
type NativePaintCascadeCandidates<T> =
    [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_PAINT_CASCADE_LAYERS];
type NativeLocalDeclarationCandidates<T> =
    [Option<CascadeValue<T>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS];
type NativeTextDeclarationCandidates<T> = [Option<CascadeValue<T>>; MAX_NATIVE_TEXT_CASCADE_LAYERS];
type NativePaintDeclarationCandidates<T> =
    [Option<CascadeValue<T>>; MAX_NATIVE_PAINT_CASCADE_LAYERS];
type NativeTextLocalCascadeCandidates<T> =
    [Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_TEXT_CASCADE_LAYERS];
type NativePhysicalLocalBorderCandidates<T> =
    [[Option<CascadeValue<LocalCascadeDeclaration<T>>>; MAX_NATIVE_LOCAL_CASCADE_LAYERS]; 4];

/// Heap-owned scratch storage for one computed-style cascade.
///
/// A page script can request a layout snapshot from a deeply nested MCP
/// future. Keeping the cascade candidates on the heap prevents the CSS
/// resolver's dozens of fixed-size candidate arrays from consuming the
/// caller's finite thread stack while retaining the same cascade semantics.
#[derive(Default)]
struct NativeCascadeScratch {
    display: NativeLocalCascadeCandidates<DisplayDeclarationValue>,
    position: NativeLocalCascadeCandidates<PositionDeclarationValue>,
    z_index: NativeLocalCascadeCandidates<ZIndexDeclarationValue>,
    pointer_events: NativeTextCascadeCandidates<PointerEventsDeclarationValue>,
    top: NativeLocalCascadeCandidates<PositionOffsetDeclarationValue>,
    right: NativeLocalCascadeCandidates<PositionOffsetDeclarationValue>,
    bottom: NativeLocalCascadeCandidates<PositionOffsetDeclarationValue>,
    left: NativeLocalCascadeCandidates<PositionOffsetDeclarationValue>,
    grid_template_columns: NativeLocalCascadeCandidates<GridTrackListDeclaration>,
    grid_template_rows: NativeLocalCascadeCandidates<GridTrackListDeclaration>,
    visibility: NativeLocalCascadeCandidates<VisibilityValue>,
    opacity: NativeLocalCascadeCandidates<OpacityDeclarationValue>,
    white_space: NativeTextCascadeCandidates<WhiteSpaceDeclarationValue>,
    text_align: NativeTextCascadeCandidates<TextAlignDeclarationValue>,
    text_align_last: NativeTextCascadeCandidates<TextAlignLastDeclarationValue>,
    text_justify: NativeTextCascadeCandidates<TextJustifyDeclarationValue>,
    direction: NativeTextCascadeCandidates<DirectionDeclarationValue>,
    justify_content: NativeLocalDeclarationCandidates<JustifyContentDeclaration>,
    align_items: NativeLocalDeclarationCandidates<AlignItemsDeclaration>,
    align_self: NativeLocalDeclarationCandidates<AlignSelfDeclaration>,
    align_content: NativeLocalDeclarationCandidates<AlignContentDeclaration>,
    flex_direction: NativeLocalDeclarationCandidates<FlexDirectionDeclaration>,
    flex_wrap: NativeLocalDeclarationCandidates<FlexWrapDeclaration>,
    flex_item_order: NativeLocalDeclarationCandidates<FlexItemOrderDeclaration>,
    flex_grow: NativeLocalDeclarationCandidates<FlexGrowDeclaration>,
    flex_shrink: NativeLocalDeclarationCandidates<FlexShrinkDeclaration>,
    flex_basis: NativeLocalDeclarationCandidates<FlexBasisDeclaration>,
    text_decoration: NativeTextDeclarationCandidates<NativeTextDecorationDeclaration>,
    text_decoration_style: NativeTextDeclarationCandidates<NativeTextDecorationStyleDeclaration>,
    text_decoration_skip_ink:
        NativeTextDeclarationCandidates<NativeTextDecorationSkipInkDeclaration>,
    text_decoration_skip_spaces:
        NativeTextDeclarationCandidates<NativeTextDecorationSkipSpacesDeclaration>,
    text_decoration_thickness:
        NativeTextDeclarationCandidates<NativeTextDecorationThicknessDeclaration>,
    text_underline_offset: NativeTextDeclarationCandidates<NativeTextUnderlineOffsetDeclaration>,
    text_decoration_color: NativePaintDeclarationCandidates<NativeTextDecorationColorDeclaration>,
    font_variant_position: NativeTextCascadeCandidates<FontVariantPositionDeclarationValue>,
    text_transform: NativeTextCascadeCandidates<TextTransformDeclarationValue>,
    font_variant_ligatures: NativeTextCascadeCandidates<FontVariantLigaturesDeclarationValue>,
    font_variant_caps: NativeTextCascadeCandidates<FontVariantCapsDeclarationValue>,
    font_variant_alternates: NativeTextCascadeCandidates<FontVariantAlternatesDeclarationValue>,
    font_language_override: NativeTextCascadeCandidates<FontLanguageOverrideDeclarationValue>,
    font_variation_settings: NativeTextCascadeCandidates<FontVariationSettingsDeclarationValue>,
    font_variant_east_asian: NativeTextCascadeCandidates<FontVariantEastAsianDeclarationValue>,
    font_variant_numeric: NativeTextCascadeCandidates<FontVariantNumericDeclarationValue>,
    font_feature_settings: NativeTextCascadeCandidates<FontFeatureSettingsDeclarationValue>,
    font_kerning: NativeTextCascadeCandidates<FontKerningDeclarationValue>,
    font_optical_sizing: NativeTextCascadeCandidates<FontOpticalSizingDeclarationValue>,
    font_palette: NativeTextCascadeCandidates<FontPaletteDeclarationValue>,
    font_weight: NativeTextCascadeCandidates<FontWeightDeclarationValue>,
    font_style: NativeTextCascadeCandidates<FontStyleDeclarationValue>,
    font_stretch: NativeTextCascadeCandidates<FontStretchDeclarationValue>,
    font_family: NativeTextCascadeCandidates<FontFamilyDeclarationValue>,
    font_size: NativeTextCascadeCandidates<NativeFontSizeDeclarationValue>,
    word_break: NativeTextCascadeCandidates<WordBreakDeclarationValue>,
    text_overflow: NativeTextLocalCascadeCandidates<TextOverflowDeclarationValue>,
    vertical_align: NativeTextCascadeCandidates<VerticalAlignDeclarationValue>,
    text_indent: NativeTextLocalCascadeCandidates<TextIndentDeclarationValue>,
    word_spacing: NativeTextCascadeCandidates<WordSpacingDeclarationValue>,
    letter_spacing: NativeTextCascadeCandidates<LetterSpacingDeclarationValue>,
    gap: GapCascade,
    width: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    height: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    min_width: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    max_width: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    min_height: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    max_height: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    line_height: NativeTextCascadeCandidates<LineHeightDeclarationValue>,
    background_color: NativePaintCascadeCandidates<NativeBackgroundColorValue>,
    background_image: NativePaintCascadeCandidates<NativeBackgroundImageValue>,
    background_repeat: NativePaintCascadeCandidates<NativeBackgroundRepeat>,
    background_position: NativePaintCascadeCandidates<NativeBackgroundPositionDeclaration>,
    background_size: NativePaintCascadeCandidates<NativeBackgroundSizeDeclaration>,
    border: NativePhysicalBorderCandidates<NativeBorderDeclaration>,
    logical_border: NativeLogicalBorderCandidates<NativeBorderDeclaration>,
    border_width: NativePhysicalBorderWidthCandidates,
    logical_border_width: NativeLogicalBorderWidthCandidates,
    border_style: NativePhysicalBorderStyleCandidates,
    logical_border_style: NativeLogicalBorderStyleCandidates,
    border_color: NativePhysicalBorderColorCandidates,
    logical_border_color: NativeLogicalBorderColorCandidates,
    border_radius: NativePhysicalBorderRadiusCandidates,
    logical_border_radius: NativeLogicalBorderRadiusCandidates,
    logical_padding: NativeLogicalBoxModelCandidates<u32>,
    logical_margin: NativeLogicalBoxModelCandidates<NativeMarginValue>,
    padding: NativePhysicalLocalBorderCandidates<PaddingDeclarationValue>,
    margin: NativePhysicalLocalBorderCandidates<MarginDeclarationValue>,
    box_sizing: NativeLocalCascadeCandidates<BoxSizingDeclarationValue>,
    color: NativePaintCascadeCandidates<NativeColorValue>,
    overflow_x: NativeLocalCascadeCandidates<OverflowDeclarationValue>,
    overflow_y: NativeLocalCascadeCandidates<OverflowDeclarationValue>,
}

impl NativeCascadeScratch {
    fn new_boxed() -> Box<Self> {
        // Initialize each field in the allocation so Rust constructs valid
        // `Option` values without first materializing the large scratch
        // object on a caller's stack.
        let mut scratch = Box::<Self>::new_uninit();
        unsafe {
            let pointer = scratch.as_mut_ptr();
            macro_rules! initialize {
                ($field:ident) => {
                    std::ptr::addr_of_mut!((*pointer).$field).write(Default::default());
                };
            }
            initialize!(display);
            initialize!(position);
            initialize!(z_index);
            initialize!(pointer_events);
            initialize!(top);
            initialize!(right);
            initialize!(bottom);
            initialize!(left);
            initialize!(grid_template_columns);
            initialize!(grid_template_rows);
            initialize!(visibility);
            initialize!(opacity);
            initialize!(white_space);
            initialize!(text_align);
            initialize!(text_align_last);
            initialize!(text_justify);
            initialize!(direction);
            initialize!(justify_content);
            initialize!(align_items);
            initialize!(align_self);
            initialize!(align_content);
            initialize!(flex_direction);
            initialize!(flex_wrap);
            initialize!(flex_item_order);
            initialize!(flex_grow);
            initialize!(flex_shrink);
            initialize!(flex_basis);
            initialize!(text_decoration);
            initialize!(text_decoration_style);
            initialize!(text_decoration_skip_ink);
            initialize!(text_decoration_skip_spaces);
            initialize!(text_decoration_thickness);
            initialize!(text_underline_offset);
            initialize!(text_decoration_color);
            initialize!(text_transform);
            initialize!(font_variant_ligatures);
            initialize!(font_variant_caps);
            initialize!(font_variant_position);
            initialize!(font_variant_alternates);
            initialize!(font_language_override);
            initialize!(font_variation_settings);
            initialize!(font_variant_east_asian);
            initialize!(font_variant_numeric);
            initialize!(font_feature_settings);
            initialize!(font_kerning);
            initialize!(font_optical_sizing);
            initialize!(font_palette);
            initialize!(font_weight);
            initialize!(font_style);
            initialize!(font_stretch);
            initialize!(font_family);
            initialize!(font_size);
            initialize!(word_break);
            initialize!(text_overflow);
            initialize!(vertical_align);
            initialize!(text_indent);
            initialize!(word_spacing);
            initialize!(letter_spacing);
            initialize!(gap);
            initialize!(width);
            initialize!(height);
            initialize!(min_width);
            initialize!(max_width);
            initialize!(min_height);
            initialize!(max_height);
            initialize!(line_height);
            initialize!(background_color);
            initialize!(background_image);
            initialize!(background_repeat);
            initialize!(background_position);
            initialize!(background_size);
            initialize!(border);
            initialize!(logical_border);
            initialize!(border_width);
            initialize!(logical_border_width);
            initialize!(border_style);
            initialize!(logical_border_style);
            initialize!(border_color);
            initialize!(logical_border_color);
            initialize!(border_radius);
            initialize!(logical_border_radius);
            initialize!(logical_padding);
            initialize!(logical_margin);
            initialize!(padding);
            initialize!(margin);
            initialize!(box_sizing);
            initialize!(color);
            initialize!(overflow_x);
            initialize!(overflow_y);
            scratch.assume_init()
        }
    }
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

mod cascade;
use cascade::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NativeLogicalBorderDeclarations {
    border: [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; LOGICAL_BORDER_SIDES],
    border_order: [usize; LOGICAL_BORDER_SIDES],
    border_important: [bool; LOGICAL_BORDER_SIDES],
    width: [Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>>; LOGICAL_BORDER_SIDES],
    width_order: [usize; LOGICAL_BORDER_SIDES],
    width_important: [bool; LOGICAL_BORDER_SIDES],
    style: [Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>>; LOGICAL_BORDER_SIDES],
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
    [[Option<CascadeValue<LocalCascadeDeclaration<BorderWidthDeclarationValue>>>;
        MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; LOGICAL_BORDER_SIDES];
type NativeLogicalBorderStyleCandidates =
    [[Option<CascadeValue<LocalCascadeDeclaration<BorderStyleDeclarationValue>>>;
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
    CascadeValue<LocalCascadeDeclaration<BorderWidthDeclarationValue>>,
>; MAX_NATIVE_BORDER_WIDTH_CASCADE_LAYERS]; 4];
type NativePhysicalBorderStyleCandidates = [[Option<
    CascadeValue<LocalCascadeDeclaration<BorderStyleDeclarationValue>>,
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
    candidate: CascadeValue<LocalCascadeDeclaration<BorderWidthDeclarationValue>>,
    layer: usize,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<BorderWidthDeclarationValue>>>;
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
    candidate: CascadeValue<LocalCascadeDeclaration<BorderStyleDeclarationValue>>,
    layer: usize,
    candidates: &mut [Option<CascadeValue<LocalCascadeDeclaration<BorderStyleDeclarationValue>>>;
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
                merge_border_width_cascade_candidate(
                    candidate,
                    layer,
                    &mut targets.width[physical_side],
                );
            }
            if let Some(candidate) = sources.style[logical_side][layer] {
                merge_border_style_cascade_candidate(
                    candidate,
                    layer,
                    &mut targets.style[physical_side],
                );
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

fn project_logical_padding_candidates(
    sources: &NativeLogicalBoxModelCandidates<u32>,
    direction: DirectionValue,
    inherited_direction: DirectionValue,
    inherited: [u32; 4],
    targets: &mut NativePhysicalLocalBorderCandidates<PaddingDeclarationValue>,
) {
    for (logical_side, logical_candidates) in sources.iter().enumerate() {
        let physical_side = logical_border_physical_side(logical_side, direction);
        let inherited_physical_side =
            logical_border_physical_side(logical_side, inherited_direction);
        for (layer, source_candidate) in logical_candidates.iter().copied().enumerate() {
            let Some(source_candidate) = source_candidate else {
                continue;
            };
            let value = match source_candidate.value {
                LocalCascadeDeclaration::Value(value) => {
                    LocalCascadeDeclaration::Value(PaddingDeclarationValue::Value(value))
                }
                LocalCascadeDeclaration::Inherit => LocalCascadeDeclaration::Value(
                    PaddingDeclarationValue::Value(inherited[inherited_physical_side]),
                ),
                LocalCascadeDeclaration::Reset => LocalCascadeDeclaration::Reset,
                LocalCascadeDeclaration::RevertLayer => LocalCascadeDeclaration::RevertLayer,
            };
            let candidate = CascadeValue {
                value,
                specificity: source_candidate.specificity,
                order: source_candidate.order,
                inline: source_candidate.inline,
            };
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

fn project_logical_margin_candidates(
    sources: &NativeLogicalBoxModelCandidates<NativeMarginValue>,
    direction: DirectionValue,
    inherited_direction: DirectionValue,
    inherited: [NativeMarginValue; 4],
    targets: &mut NativePhysicalLocalBorderCandidates<MarginDeclarationValue>,
) {
    for (logical_side, logical_candidates) in sources.iter().enumerate() {
        let physical_side = logical_border_physical_side(logical_side, direction);
        let inherited_physical_side =
            logical_border_physical_side(logical_side, inherited_direction);
        for (layer, source_candidate) in logical_candidates.iter().copied().enumerate() {
            let Some(source_candidate) = source_candidate else {
                continue;
            };
            let value = match source_candidate.value {
                LocalCascadeDeclaration::Value(value) => {
                    LocalCascadeDeclaration::Value(MarginDeclarationValue::Value(value))
                }
                LocalCascadeDeclaration::Inherit => LocalCascadeDeclaration::Value(
                    MarginDeclarationValue::Value(inherited[inherited_physical_side]),
                ),
                LocalCascadeDeclaration::Reset => LocalCascadeDeclaration::Reset,
                LocalCascadeDeclaration::RevertLayer => LocalCascadeDeclaration::RevertLayer,
            };
            let candidate = CascadeValue {
                value,
                specificity: source_candidate.specificity,
                order: source_candidate.order,
                inline: source_candidate.inline,
            };
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
            border.and_then(project_logical_border_width_declaration),
            specificity,
            order,
            inline,
            important,
            &mut width[index],
        );
        apply_paint_cascade_declaration(
            border.and_then(project_logical_border_style_declaration),
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
    position: bool,
    z_index: bool,
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
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
    pointer_events: bool,
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
    font_variant_ligatures: bool,
    font_variant_caps: bool,
    font_variant_position: bool,
    font_variant_alternates: bool,
    font_language_override: bool,
    font_variation_settings: bool,
    font_variant_east_asian: bool,
    font_variant_numeric: bool,
    font_feature_settings: bool,
    font_kerning: bool,
    font_optical_sizing: bool,
    font_palette: bool,
    font_weight: bool,
    font_style: bool,
    font_stretch: bool,
    font_family: bool,
    font_size: bool,
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
    display: Option<LocalCascadeDeclaration<DisplayDeclarationValue>>,
    position: Option<LocalCascadeDeclaration<PositionDeclarationValue>>,
    z_index: Option<LocalCascadeDeclaration<ZIndexDeclarationValue>>,
    pointer_events: Option<InheritedTextDeclaration<PointerEventsDeclarationValue>>,
    top: Option<LocalCascadeDeclaration<PositionOffsetDeclarationValue>>,
    right: Option<LocalCascadeDeclaration<PositionOffsetDeclarationValue>>,
    bottom: Option<LocalCascadeDeclaration<PositionOffsetDeclarationValue>>,
    left: Option<LocalCascadeDeclaration<PositionOffsetDeclarationValue>>,
    grid_template_columns: Option<LocalCascadeDeclaration<GridTrackListDeclaration>>,
    grid_template_columns_important: bool,
    grid_template_rows: Option<LocalCascadeDeclaration<GridTrackListDeclaration>>,
    grid_template_rows_important: bool,
    visibility: Option<LocalCascadeDeclaration<VisibilityValue>>,
    opacity: Option<LocalCascadeDeclaration<OpacityDeclarationValue>>,
    local_importance: NativeLocalDeclarationImportance,
    dimension_importance: NativeDimensionDeclarationImportance,
    box_model_importance: NativeBoxModelDeclarationImportance,
    flex_importance: NativeFlexDeclarationImportance,
    text_importance: NativeTextDeclarationImportance,
    white_space: Option<InheritedTextDeclaration<WhiteSpaceDeclarationValue>>,
    text_align: Option<InheritedTextDeclaration<TextAlignDeclarationValue>>,
    text_align_last: Option<InheritedTextDeclaration<TextAlignLastDeclarationValue>>,
    text_justify: Option<InheritedTextDeclaration<TextJustifyDeclarationValue>>,
    justify_content: Option<JustifyContentDeclaration>,
    align_items: Option<AlignItemsDeclaration>,
    align_self: Option<AlignSelfDeclaration>,
    align_content: Option<AlignContentDeclaration>,
    flex_direction: Option<FlexDirectionDeclaration>,
    direction: Option<InheritedTextDeclaration<DirectionDeclarationValue>>,
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
    text_transform: Option<InheritedTextDeclaration<TextTransformDeclarationValue>>,
    font_variant_ligatures: Option<InheritedTextDeclaration<FontVariantLigaturesDeclarationValue>>,
    font_variant_caps: Option<InheritedTextDeclaration<FontVariantCapsDeclarationValue>>,
    font_variant_position: Option<InheritedTextDeclaration<FontVariantPositionDeclarationValue>>,
    font_variant_alternates:
        Option<InheritedTextDeclaration<FontVariantAlternatesDeclarationValue>>,
    font_language_override: Option<InheritedTextDeclaration<FontLanguageOverrideDeclarationValue>>,
    font_variation_settings:
        Option<InheritedTextDeclaration<FontVariationSettingsDeclarationValue>>,
    font_variant_east_asian: Option<InheritedTextDeclaration<FontVariantEastAsianDeclarationValue>>,
    font_variant_numeric: Option<InheritedTextDeclaration<FontVariantNumericDeclarationValue>>,
    font_feature_settings: Option<InheritedTextDeclaration<FontFeatureSettingsDeclarationValue>>,
    font_kerning: Option<InheritedTextDeclaration<FontKerningDeclarationValue>>,
    font_optical_sizing: Option<InheritedTextDeclaration<FontOpticalSizingDeclarationValue>>,
    font_palette: Option<InheritedTextDeclaration<FontPaletteDeclarationValue>>,
    font_weight: Option<InheritedTextDeclaration<FontWeightDeclarationValue>>,
    font_style: Option<InheritedTextDeclaration<FontStyleDeclarationValue>>,
    font_stretch: Option<InheritedTextDeclaration<FontStretchDeclarationValue>>,
    font_family: Option<InheritedTextDeclaration<FontFamilyDeclarationValue>>,
    font_size: Option<InheritedTextDeclaration<NativeFontSizeDeclarationValue>>,
    word_break: Option<InheritedTextDeclaration<WordBreakDeclarationValue>>,
    text_overflow: Option<LocalCascadeDeclaration<TextOverflowDeclarationValue>>,
    vertical_align: Option<InheritedTextDeclaration<VerticalAlignDeclarationValue>>,
    text_indent: Option<LocalCascadeDeclaration<TextIndentDeclarationValue>>,
    word_spacing: Option<InheritedTextDeclaration<WordSpacingDeclarationValue>>,
    letter_spacing: Option<InheritedTextDeclaration<LetterSpacingDeclarationValue>>,
    gap: Option<GapShorthandDeclaration>,
    gap_order: usize,
    gap_important: bool,
    row_gap: Option<GapComponentDeclaration>,
    row_gap_order: usize,
    row_gap_important: bool,
    column_gap: Option<GapComponentDeclaration>,
    column_gap_order: usize,
    column_gap_important: bool,
    width: Option<LocalCascadeDeclaration<DimensionDeclarationValue>>,
    height: Option<LocalCascadeDeclaration<DimensionDeclarationValue>>,
    min_width: Option<LocalCascadeDeclaration<DimensionDeclarationValue>>,
    max_width: Option<LocalCascadeDeclaration<DimensionDeclarationValue>>,
    min_height: Option<LocalCascadeDeclaration<DimensionDeclarationValue>>,
    max_height: Option<LocalCascadeDeclaration<DimensionDeclarationValue>>,
    line_height: Option<InheritedTextDeclaration<LineHeightDeclarationValue>>,
    background_color: Option<LocalCascadeDeclaration<NativeBackgroundColorValue>>,
    background_color_important: bool,
    background_image: Option<LocalCascadeDeclaration<NativeBackgroundImageValue>>,
    background_image_important: bool,
    background_repeat: Option<LocalCascadeDeclaration<NativeBackgroundRepeat>>,
    background_repeat_important: bool,
    background_position: Option<LocalCascadeDeclaration<NativeBackgroundPositionDeclaration>>,
    background_position_important: bool,
    background_size: Option<LocalCascadeDeclaration<NativeBackgroundSizeDeclaration>>,
    background_size_important: bool,
    border: [Option<LocalCascadeDeclaration<NativeBorderDeclaration>>; 4],
    border_order: [usize; 4],
    border_important: [bool; 4],
    logical_border: NativeLogicalBorderDeclarations,
    border_width: [Option<LocalCascadeDeclaration<BorderWidthDeclarationValue>>; 4],
    border_width_order: [usize; 4],
    border_width_important: [bool; 4],
    border_style: [Option<LocalCascadeDeclaration<BorderStyleDeclarationValue>>; 4],
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
    padding: [Option<LocalCascadeDeclaration<PaddingDeclarationValue>>; 4],
    padding_order: [usize; 4],
    margin: [Option<LocalCascadeDeclaration<MarginDeclarationValue>>; 4],
    margin_order: [usize; 4],
    box_sizing: Option<LocalCascadeDeclaration<BoxSizingDeclarationValue>>,
    color: Option<LocalCascadeDeclaration<NativeColorValue>>,
    color_important: bool,
    overflow: Option<LocalCascadeDeclaration<OverflowDeclarationValue>>,
    overflow_x: Option<LocalCascadeDeclaration<OverflowDeclarationValue>>,
    overflow_y: Option<LocalCascadeDeclaration<OverflowDeclarationValue>>,
    overflow_importance: NativeOverflowDeclarationImportance,
    logical_box_model: NativeLogicalBoxModelDeclarations,
}

fn apply_background_shorthand_declaration(
    declarations: &mut NativeDeclarations,
    shorthand: NativeBackgroundShorthandDeclaration,
    important: bool,
) {
    let (color, image, repeat, position, size) = match shorthand {
        NativeBackgroundShorthandDeclaration::Value(value) => (
            LocalCascadeDeclaration::Value(value.color),
            LocalCascadeDeclaration::Value(value.image),
            LocalCascadeDeclaration::Value(value.repeat),
            LocalCascadeDeclaration::Value(NativeBackgroundPositionDeclaration::Value(
                value.position,
            )),
            LocalCascadeDeclaration::Value(NativeBackgroundSizeDeclaration::Value(value.size)),
        ),
        NativeBackgroundShorthandDeclaration::CustomProperty(name_hash) => (
            LocalCascadeDeclaration::Value(NativeBackgroundColorValue::CustomPropertyShorthand(
                name_hash,
            )),
            LocalCascadeDeclaration::Value(NativeBackgroundImageValue::CustomPropertyShorthand(
                name_hash,
            )),
            LocalCascadeDeclaration::Value(NativeBackgroundRepeat::CustomPropertyShorthand(
                name_hash,
            )),
            LocalCascadeDeclaration::Value(
                NativeBackgroundPositionDeclaration::CustomPropertyShorthand(name_hash),
            ),
            LocalCascadeDeclaration::Value(
                NativeBackgroundSizeDeclaration::CustomPropertyShorthand(name_hash),
            ),
        ),
        NativeBackgroundShorthandDeclaration::CustomPropertyFallback(name_hash, value) => {
            let color = match value.color {
                NativeBackgroundColorValue::Color(color) => {
                    NativeBackgroundColorShorthandFallback::Color(color)
                }
                NativeBackgroundColorValue::CurrentColor => {
                    NativeBackgroundColorShorthandFallback::CurrentColor
                }
                _ => NativeBackgroundColorShorthandFallback::Color(NativeColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                }),
            };
            let image = match value.image {
                NativeBackgroundImageValue::Url(source) => Some(source),
                NativeBackgroundImageValue::None => None,
                _ => None,
            };
            let repeat = match value.repeat {
                NativeBackgroundRepeat::Repeat => NativeBackgroundRepeatFallback::Repeat,
                NativeBackgroundRepeat::RepeatX => NativeBackgroundRepeatFallback::RepeatX,
                NativeBackgroundRepeat::RepeatY => NativeBackgroundRepeatFallback::RepeatY,
                NativeBackgroundRepeat::NoRepeat => NativeBackgroundRepeatFallback::NoRepeat,
                _ => NativeBackgroundRepeatFallback::Repeat,
            };
            (
                LocalCascadeDeclaration::Value(
                    NativeBackgroundColorValue::CustomPropertyShorthandFallback(name_hash, color),
                ),
                LocalCascadeDeclaration::Value(
                    NativeBackgroundImageValue::CustomPropertyShorthandFallback(name_hash, image),
                ),
                LocalCascadeDeclaration::Value(
                    NativeBackgroundRepeat::CustomPropertyShorthandFallback(name_hash, repeat),
                ),
                LocalCascadeDeclaration::Value(
                    NativeBackgroundPositionDeclaration::CustomPropertyShorthandFallback(
                        name_hash,
                        value.position,
                    ),
                ),
                LocalCascadeDeclaration::Value(
                    NativeBackgroundSizeDeclaration::CustomPropertyShorthandFallback(
                        name_hash, value.size,
                    ),
                ),
            )
        }
        NativeBackgroundShorthandDeclaration::Inherit => (
            LocalCascadeDeclaration::Inherit,
            LocalCascadeDeclaration::Inherit,
            LocalCascadeDeclaration::Inherit,
            LocalCascadeDeclaration::Inherit,
            LocalCascadeDeclaration::Inherit,
        ),
        NativeBackgroundShorthandDeclaration::Reset => (
            LocalCascadeDeclaration::Reset,
            LocalCascadeDeclaration::Reset,
            LocalCascadeDeclaration::Reset,
            LocalCascadeDeclaration::Reset,
            LocalCascadeDeclaration::Reset,
        ),
        NativeBackgroundShorthandDeclaration::RevertLayer => (
            LocalCascadeDeclaration::RevertLayer,
            LocalCascadeDeclaration::RevertLayer,
            LocalCascadeDeclaration::RevertLayer,
            LocalCascadeDeclaration::RevertLayer,
            LocalCascadeDeclaration::RevertLayer,
        ),
    };
    declarations.background_color = Some(color);
    declarations.background_color_important = important;
    declarations.background_image = Some(image);
    declarations.background_image_important = important;
    declarations.background_repeat = Some(repeat);
    declarations.background_repeat_important = important;
    declarations.background_position = Some(position);
    declarations.background_position_important = important;
    declarations.background_size = Some(size);
    declarations.background_size_important = important;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCustomPropertyDeclaration {
    name_hash: u64,
    value: String,
    important: bool,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCustomPropertyCandidate {
    value: String,
    specificity: u16,
    order: usize,
    important: bool,
    inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeStyleRule {
    selector: NativeSelector,
    declarations: NativeDeclarations,
    custom_properties: Vec<NativeCustomPropertyDeclaration>,
    media: Option<String>,
    supports: Option<String>,
    order: usize,
}

/// A bounded index over each selector's rightmost simple selector. Most page
/// styles use class/id/tag selectors, so unrelated rules can be skipped while
/// retaining the full selector checks for candidates. Universal and complex
/// selectors remain in the fallback list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeStyleRuleIndex {
    by_id: BTreeMap<String, Vec<usize>>,
    by_class: BTreeMap<String, Vec<usize>>,
    by_tag: BTreeMap<String, Vec<usize>>,
    fallback: Vec<usize>,
}

impl NativeStyleRuleIndex {
    fn build(rules: &[NativeStyleRule]) -> Self {
        Self::build_selected(rules, 0..rules.len())
    }

    fn build_selected(
        rules: &[NativeStyleRule],
        selected: impl IntoIterator<Item = usize>,
    ) -> Self {
        let mut index = Self::default();
        for rule_index in selected {
            let rule = &rules[rule_index];
            let Some(target) = rule.selector.compounds.last() else {
                index.fallback.push(rule_index);
                continue;
            };
            if let Some(id) = target.id.as_ref() {
                index.by_id.entry(id.clone()).or_default().push(rule_index);
            } else if let Some(class) = target.classes.first() {
                index
                    .by_class
                    .entry(class.clone())
                    .or_default()
                    .push(rule_index);
            } else if let Some(tag) = target.tag.as_ref() {
                index
                    .by_tag
                    .entry(tag.clone())
                    .or_default()
                    .push(rule_index);
            } else {
                index.fallback.push(rule_index);
            }
        }
        index
    }

    fn candidates(&self, node: &NativeNode) -> Vec<usize> {
        let mut candidates = self.fallback.clone();
        if let Some(id) = node.attribute("id")
            && let Some(indices) = self.by_id.get(id)
        {
            candidates.extend_from_slice(indices);
        }
        if let Some(classes) = node.attribute("class") {
            for class in classes.split_ascii_whitespace() {
                if let Some(indices) = self.by_class.get(class) {
                    candidates.extend_from_slice(indices);
                }
            }
        }
        if let Some(tag) = node.element_name()
            && let Some(indices) = self.by_tag.get(tag)
        {
            candidates.extend_from_slice(indices);
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }
}

pub(in crate::native_engine) fn strip_important_suffix(value: &str) -> (&str, bool) {
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

mod rule_parser;
pub(in crate::native_engine::css) use rule_parser::{NativeCssParseOutput, parse_source};
#[cfg(test)]
pub(in crate::native_engine::css) use rule_parser::{
    parse_font_palette_overrides, parse_namespace_statement,
};

#[path = "css/declarations.rs"]
mod declarations;
pub(crate) use declarations::collect_declaration_diagnostics;
use declarations::*;

pub(crate) fn selector_matches_in_document(
    document: &NativeDocument,
    source: &str,
) -> Result<Vec<NativeNodeId>, NativeEngineError> {
    let selector = parse_selector(source).ok_or_else(|| {
        NativeEngineError::invalid(
            "CSS action locator",
            "selector is malformed or outside the bounded native selector grammar",
        )
    })?;
    Ok(document
        .element_node_ids()
        .filter(|node_id| selector.matches_in_document(document, *node_id))
        .collect())
}

mod stylesheet_utils;
use stylesheet_utils::*;
pub(crate) use stylesheet_utils::{css_import_matches, decode_css_url_value};

mod property_parser;
use property_parser::*;
pub(crate) use property_parser::{
    font_family_hash, format_font_face_unicode_ranges, format_font_feature_settings,
    format_font_metric_override, format_font_size_adjust, format_font_stretch_range,
    format_font_variation_settings, parse_color, parse_font_face_unicode_range,
    parse_font_face_variant, parse_font_feature_settings, parse_font_metric_override,
    parse_font_size_adjust, parse_font_stretch_range, parse_font_variation_settings,
    parse_font_weight_range,
};
mod selector_matching;
use selector_matching::*;
mod selector_parser;
use selector_parser::*;

#[cfg(test)]
#[path = "css/tests.rs"]
mod tests;
