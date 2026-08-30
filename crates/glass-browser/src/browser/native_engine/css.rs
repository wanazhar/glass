use super::config::MAX_NATIVE_VIEWPORT_DIMENSION;
use super::dom::NativeNode;
use super::error::NativeEngineError;

pub(crate) const MAX_NATIVE_STYLE_RULES: usize = 512;
const MAX_SELECTOR_BYTES: usize = 256;

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
    Other,
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
    width: Option<u32>,
    height: Option<u32>,
    line_height: Option<u32>,
    background_color: Option<NativeColor>,
    border: Option<NativeBorder>,
    border_radius: NativeBorderRadius,
    padding: NativeBoxEdges,
    margin: NativeBoxEdges,
    box_sizing: NativeBoxSizing,
    color: Option<NativeColor>,
    overflow_hidden: bool,
}

impl NativeComputedStyle {
    pub(crate) const fn hidden(self) -> bool {
        matches!(self.display, DisplayValue::None) || self.visibility_hidden
    }

    pub(crate) const fn display(self) -> DisplayValue {
        self.display
    }

    pub(crate) const fn width(self) -> Option<u32> {
        self.width
    }

    pub(crate) const fn height(self) -> Option<u32> {
        self.height
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

    pub(crate) const fn overflow_hidden(self) -> bool {
        self.overflow_hidden
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NativeStylesheet {
    rules: Vec<NativeStyleRule>,
}

impl NativeStylesheet {
    pub(crate) fn from_sources(
        sources: impl IntoIterator<Item = String>,
    ) -> Result<Self, NativeEngineError> {
        let mut stylesheet = Self::default();
        let mut order = 0;
        for source in sources {
            parse_source(&source, &mut stylesheet.rules, &mut order)?;
        }
        Ok(stylesheet)
    }

    pub(crate) fn computed_for(&self, node: &NativeNode) -> NativeComputedStyle {
        self.computed_for_with_inherited_color(node, None)
    }

    pub(crate) fn computed_for_with_inherited_color(
        &self,
        node: &NativeNode,
        inherited_color: Option<NativeColor>,
    ) -> NativeComputedStyle {
        let mut display = None;
        let mut visibility = None;
        let mut width = None;
        let mut height = None;
        let mut line_height = None;
        let mut background_color = None;
        let mut border: [Option<CascadeValue<NativeBorderSide>>; 4] = [None; 4];
        let mut border_radius = None;
        let mut padding: [Option<CascadeValue<u32>>; 4] = [None; 4];
        let mut margin: [Option<CascadeValue<u32>>; 4] = [None; 4];
        let mut box_sizing = None;
        let mut color = None;
        let mut overflow = None;
        for rule in &self.rules {
            if !rule.selector.matches(node) {
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
            if let Some(value) = rule.declarations.overflow
                && wins(rule.selector.specificity, rule.order, false, overflow)
            {
                overflow = Some(CascadeValue {
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
            if let Some(value) = declarations.overflow
                && wins(u16::MAX, usize::MAX, true, overflow)
            {
                overflow = Some(CascadeValue {
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
            width: width.map(|value| value.value),
            height: height.map(|value| value.value),
            line_height: line_height.map(|value| value.value),
            background_color: background_color.map(|value| value.value),
            border: NativeBorder::from_sides(border.map(|value| value.map(|value| value.value))),
            border_radius: border_radius.map_or(NativeBorderRadius::default(), |value| value.value),
            padding: NativeBoxEdges::from_cascade(padding),
            margin: NativeBoxEdges::from_cascade(margin),
            box_sizing: box_sizing.map_or(NativeBoxSizing::ContentBox, |value| value.value),
            color: color.map(|value| value.value).or(inherited_color),
            overflow_hidden: overflow.is_some_and(|value| value.value == OverflowValue::Hidden),
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
    width: Option<u32>,
    height: Option<u32>,
    line_height: Option<u32>,
    background_color: Option<NativeColor>,
    border: [Option<NativeBorderSide>; 4],
    border_radius: Option<NativeBorderRadius>,
    padding: [Option<u32>; 4],
    margin: [Option<u32>; 4],
    box_sizing: Option<NativeBoxSizing>,
    color: Option<NativeColor>,
    overflow: Option<OverflowValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeStyleRule {
    selector: NativeSelector,
    declarations: NativeDeclarations,
    order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<NativeAttributeSelector>,
    specificity: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeAttributeSelector {
    name: String,
    value: Option<String>,
}

impl NativeSelector {
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
) -> Result<(), NativeEngineError> {
    let source = strip_comments(source);
    let mut cursor = 0;
    while let Some(open_relative) = source[cursor..].find('{') {
        let open = cursor + open_relative;
        let Some(close_relative) = source[open + 1..].find('}') else {
            break;
        };
        let close = open + 1 + close_relative;
        let declarations = parse_declarations(&source[open + 1..close]);
        if declarations.display.is_some()
            || declarations.visibility.is_some()
            || declarations.width.is_some()
            || declarations.height.is_some()
            || declarations.line_height.is_some()
            || declarations.background_color.is_some()
            || declarations.border.iter().any(Option::is_some)
            || declarations.border_radius.is_some()
            || declarations.padding.iter().any(Option::is_some)
            || declarations.margin.iter().any(Option::is_some)
            || declarations.box_sizing.is_some()
            || declarations.color.is_some()
            || declarations.overflow.is_some()
        {
            for selector_text in source[cursor..open].split(',') {
                let Some(selector) = parse_selector(selector_text) else {
                    continue;
                };
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
        }
        cursor = close + 1;
    }
    Ok(())
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
            "width" => {
                declarations.width = parse_dimension(value);
            }
            "height" => {
                declarations.height = parse_dimension(value);
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
                declarations.overflow = parse_overflow(value);
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
        _ => None,
    }
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
        "flex" | "grid" => Some(DisplayValue::Other),
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
        "visible" | "auto" | "scroll" => Some(OverflowValue::Other),
        _ => None,
    }
}

fn parse_selector(source: &str) -> Option<NativeSelector> {
    let source = source.trim();
    if source.is_empty()
        || source.len() > MAX_SELECTOR_BYTES
        || source.chars().any(char::is_whitespace)
    {
        return None;
    }
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut selector = NativeSelector {
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
    fn selector_parser_supports_one_compound_selector() {
        let selector = parse_selector("button.primary[data-state=ready]").unwrap();
        assert_eq!(selector.specificity, 21);
        assert!(parse_selector("main > button").is_none());
        assert!(parse_selector("button:hover").is_none());
    }

    #[test]
    fn declarations_parse_only_supported_presentation_properties() {
        let declarations = parse_declarations(
            "color: red; display: none !important; visibility: visible; width: 240px; height: 30px; line-height: 28px; border: 2px solid #102030; border-radius: 1px 2px 3px 4px; padding: 4px; margin: 3px; box-sizing: border-box; overflow: hidden",
        );
        assert_eq!(declarations.display, Some(DisplayValue::None));
        assert_eq!(declarations.visibility, Some(VisibilityValue::Other));
        assert_eq!(declarations.width, Some(240));
        assert_eq!(declarations.height, Some(30));
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
        assert_eq!(parse_overflow("scroll"), Some(OverflowValue::Other));
        assert_eq!(parse_overflow("clip"), None);
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
        assert_eq!(parse_color("rgba(1, 2, 3, 0.5)"), None);
        assert_eq!(parse_color("rgb(101%, 2, 3)"), None);
    }
}
