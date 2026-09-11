use super::config::MAX_NATIVE_NODES;
use super::css::{
    FontStyleValue, FontWeightValue, NativeBorderRadius, NativeBorderStyle, NativeColor,
    NativeTextDecorationSkipInk, NativeTextDecorationSkipSpaces, NativeTextDecorationStyle,
};
use super::dom::{NativeDocument, NativeNode, NativeNodeId};
use super::error::NativeEngineError;
use super::layout::{
    MAX_NATIVE_SVG_POINTS, NativeLayoutPaintOrder, NativeLayoutSnapshot, NativePoint, NativeRect,
    NativeSvgSubpath, svg_line_points, svg_path_subpaths, svg_points,
};

/// Maximum number of immutable commands retained in one native display list.
pub const MAX_NATIVE_DISPLAY_COMMANDS: usize = MAX_NATIVE_NODES.saturating_mul(4).saturating_add(1);

/// One physical border side carried by a native display-list border command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeBorderPaintSide {
    pub width: u32,
    pub style: NativeBorderStyle,
    pub color: NativeColor,
}

/// Bounded physical border paint data for the top, right, bottom, and left
/// sides of one outer layout box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeBorderPaint {
    pub top: NativeBorderPaintSide,
    pub right: NativeBorderPaintSide,
    pub bottom: NativeBorderPaintSide,
    pub left: NativeBorderPaintSide,
}

impl NativeBorderPaint {
    pub(crate) fn from_style(border: super::css::NativeBorder) -> Self {
        Self {
            top: Self::side_from_style(border.top()),
            right: Self::side_from_style(border.right()),
            bottom: Self::side_from_style(border.bottom()),
            left: Self::side_from_style(border.left()),
        }
    }

    const fn side_from_style(side: super::css::NativeBorderSide) -> NativeBorderPaintSide {
        NativeBorderPaintSide {
            width: side.width(),
            style: side.style(),
            color: side.color(),
        }
    }
}

/// The bounded SVG shape families understood by the native stroke rasterizer.
///
/// `Ellipse` covers both SVG `circle` and `ellipse` elements because layout
/// has already normalized both into an axis-aligned bounding rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeSvgStrokeShape {
    Rect,
    Ellipse,
}

/// One bounded command consumed by the native software rasterizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeDisplayCommand {
    BeginOpacityGroup {
        node_id: NativeNodeId,
        opacity: u8,
    },
    Clear {
        color: NativeColor,
    },
    FillRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        radius: NativeBorderRadius,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    SvgStroke {
        node_id: NativeNodeId,
        shape: NativeSvgStrokeShape,
        rect: NativeRect,
        width: u32,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    SvgPolygonFill {
        node_id: NativeNodeId,
        rect: NativeRect,
        points: Vec<NativePoint>,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    SvgPolyline {
        node_id: NativeNodeId,
        rect: NativeRect,
        points: Vec<NativePoint>,
        closed: bool,
        width: u32,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    SvgPathFill {
        node_id: NativeNodeId,
        rect: NativeRect,
        subpaths: Vec<NativeSvgSubpath>,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    SvgPathStroke {
        node_id: NativeNodeId,
        rect: NativeRect,
        subpaths: Vec<NativeSvgSubpath>,
        width: u32,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    BorderRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        radius: NativeBorderRadius,
        borders: NativeBorderPaint,
        clip: Option<NativeRect>,
    },
    TextRun {
        node_id: NativeNodeId,
        origin: NativePoint,
        text: String,
        truncated: bool,
        color: NativeColor,
        decoration_color: NativeColor,
        decoration_style: NativeTextDecorationStyle,
        decoration_skip_ink: NativeTextDecorationSkipInk,
        decoration_skip_spaces: NativeTextDecorationSkipSpaces,
        decoration_thickness: u32,
        underline_offset: i32,
        underline: bool,
        overline: bool,
        line_through: bool,
        bold: bool,
        italic: bool,
        word_spacing: u32,
        letter_spacing: u32,
        justify_spacing: u32,
        clip: Option<NativeRect>,
    },
    EndOpacityGroup {
        node_id: NativeNodeId,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeTextLineBoundary {
    pub starts_line: bool,
    pub ends_line: bool,
}

/// Immutable display-list projection for one native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDisplayList {
    pub revision: u64,
    pub viewport: super::config::Viewport,
    /// The root viewport offset used when replaying document-space commands.
    pub scroll_offset: NativePoint,
    pub commands: Vec<NativeDisplayCommand>,
    /// Immutable line-edge metadata in text-command order.
    pub(crate) text_run_boundaries: Vec<NativeTextLineBoundary>,
}

impl NativeDisplayList {
    pub(crate) fn build(
        document: &NativeDocument,
        layout: &NativeLayoutSnapshot,
    ) -> Result<Self, NativeEngineError> {
        if layout.revision != document.revision() {
            return Err(NativeEngineError::invalid(
                "layout snapshot",
                "layout revision does not match the current native document",
            ));
        }
        let mut commands = Vec::with_capacity(
            layout
                .boxes
                .len()
                .saturating_add(layout.text_runs.len())
                .saturating_add(1),
        );
        let mut text_run_boundaries = Vec::with_capacity(layout.text_runs.len());
        push_command(
            &mut commands,
            NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
        )?;
        for entry in &layout.paint_order {
            match *entry {
                NativeLayoutPaintOrder::BeginOpacityGroup { node_id, opacity } => {
                    push_command(
                        &mut commands,
                        NativeDisplayCommand::BeginOpacityGroup { node_id, opacity },
                    )?;
                }
                NativeLayoutPaintOrder::Box(box_index) => {
                    let layout_box = layout.boxes.get(box_index).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "layout paint order",
                            "box entry is missing from the layout snapshot",
                        )
                    })?;
                    let style = document.computed_style_for_layout(layout_box.node_id);
                    let clip = paint_clip(document, layout, layout_box.node_id);
                    if let Some(color) = style.background_color() {
                        push_command(
                            &mut commands,
                            NativeDisplayCommand::FillRect {
                                node_id: layout_box.node_id,
                                rect: layout_box.rect,
                                radius: style.border_radius(),
                                color,
                                clip,
                            },
                        )?;
                    }
                    if let Some(border) = style.border()
                        && border.any_width()
                    {
                        let borders = NativeBorderPaint::from_style(border);
                        push_command(
                            &mut commands,
                            NativeDisplayCommand::BorderRect {
                                node_id: layout_box.node_id,
                                rect: layout_box.rect,
                                radius: style.border_radius(),
                                borders,
                                clip,
                            },
                        )?;
                    }
                    for command in
                        svg_paint_commands(document, layout_box.node_id, layout_box.rect, clip)
                    {
                        push_command(&mut commands, command)?;
                    }
                }
                NativeLayoutPaintOrder::Text(text_index) => {
                    let text_run = layout.text_runs.get(text_index).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "layout paint order",
                            "text entry is missing from the layout snapshot",
                        )
                    })?;
                    if text_run.text.is_empty() {
                        continue;
                    }
                    let style = document.computed_style_for_layout(text_run.node_id);
                    let decoration = style.text_decoration();
                    let color = style.color().unwrap_or(NativeColor::BLACK);
                    let decoration_color = style.text_decoration_color().unwrap_or(color);
                    let clip = paint_clip(document, layout, text_run.node_id);
                    push_command(
                        &mut commands,
                        NativeDisplayCommand::TextRun {
                            node_id: text_run.node_id,
                            origin: text_run.origin,
                            text: text_run.text.clone(),
                            truncated: text_run.truncated,
                            color,
                            decoration_color,
                            decoration_style: style.text_decoration_style(),
                            decoration_skip_ink: style.text_decoration_skip_ink(),
                            decoration_skip_spaces: style.text_decoration_skip_spaces(),
                            decoration_thickness: style.text_decoration_thickness(),
                            underline_offset: style.text_underline_offset(),
                            underline: decoration.underline(),
                            overline: decoration.overline(),
                            line_through: decoration.line_through(),
                            bold: style.font_weight() == FontWeightValue::Bold,
                            italic: style.font_style() == FontStyleValue::Italic,
                            word_spacing: style.word_spacing(),
                            letter_spacing: style.letter_spacing(),
                            justify_spacing: text_run.justify_spacing,
                            clip,
                        },
                    )?;
                    text_run_boundaries.push(NativeTextLineBoundary {
                        starts_line: text_run.starts_line,
                        ends_line: text_run.ends_line,
                    });
                }
                NativeLayoutPaintOrder::EndOpacityGroup { node_id } => {
                    push_command(
                        &mut commands,
                        NativeDisplayCommand::EndOpacityGroup { node_id },
                    )?;
                }
            }
        }
        Ok(Self {
            revision: document.revision(),
            viewport: layout.viewport,
            scroll_offset: layout.scroll_offset,
            commands,
            text_run_boundaries,
        })
    }
}

const MAX_NATIVE_SVG_SCANLINES: u32 = 1024;
const MAX_NATIVE_SVG_STROKE_WIDTH: u32 = 32;

fn svg_paint_commands(
    document: &NativeDocument,
    node_id: NativeNodeId,
    bounds: NativeRect,
    clip: Option<NativeRect>,
) -> Vec<NativeDisplayCommand> {
    let Some(node) = document.node(node_id) else {
        return Vec::new();
    };
    let Some(shape) = node.element_name() else {
        return Vec::new();
    };
    if !matches!(
        shape,
        "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path"
    ) || bounds.width == 0
        || bounds.height == 0
    {
        return Vec::new();
    }
    let points = match shape {
        "line" => Some(svg_line_points(node)),
        "polyline" | "polygon" => svg_points(node),
        _ => None,
    };
    let subpaths = (shape == "path").then(|| svg_path_subpaths(node)).flatten();
    if shape == "path"
        && subpaths
            .as_ref()
            .is_none_or(|subpaths| subpaths.iter().all(|subpath| subpath.points.is_empty()))
    {
        return Vec::new();
    }
    if matches!(shape, "line" | "polyline" | "polygon")
        && points
            .as_ref()
            .is_none_or(|points| points.len() < 2 || points.len() > MAX_NATIVE_SVG_POINTS)
    {
        return Vec::new();
    }
    let mut commands = Vec::new();
    let fill = svg_presentation_value(node, "fill").unwrap_or("black");
    if !fill.eq_ignore_ascii_case("none") {
        let color = svg_paint_color(document, node_id, fill).unwrap_or(NativeColor::BLACK);
        if shape == "rect" {
            commands.push(NativeDisplayCommand::FillRect {
                node_id,
                rect: bounds,
                radius: NativeBorderRadius::default(),
                color,
                clip,
            });
        } else if matches!(shape, "circle" | "ellipse") {
            let rows = bounds.height.min(MAX_NATIVE_SVG_SCANLINES);
            commands.reserve(rows as usize);
            for row in 0..rows {
                let top = bounds.y.saturating_add(
                    u32::try_from(u64::from(row) * u64::from(bounds.height) / u64::from(rows))
                        .unwrap_or(u32::MAX),
                );
                let bottom = bounds.y.saturating_add(
                    u32::try_from(
                        u64::from(row.saturating_add(1)) * u64::from(bounds.height)
                            / u64::from(rows),
                    )
                    .unwrap_or(u32::MAX),
                );
                if bottom <= top {
                    continue;
                }
                let normalized_y = (f64::from(row) + 0.5) / f64::from(rows) * 2.0 - 1.0;
                let horizontal = (1.0 - normalized_y * normalized_y).max(0.0).sqrt();
                let center = f64::from(bounds.width) / 2.0;
                let half_width = center * horizontal;
                let left = half_width.mul_add(-1.0, center).floor().max(0.0) as u32;
                let right = half_width
                    .mul_add(1.0, center)
                    .ceil()
                    .min(f64::from(bounds.width)) as u32;
                if right <= left {
                    continue;
                }
                commands.push(NativeDisplayCommand::FillRect {
                    node_id,
                    rect: NativeRect {
                        x: bounds.x.saturating_add(left),
                        y: top,
                        width: right.saturating_sub(left),
                        height: bottom.saturating_sub(top),
                    },
                    radius: NativeBorderRadius::default(),
                    color,
                    clip,
                });
            }
        } else if shape == "polygon" {
            if let Some(points) = points.as_ref() {
                commands.push(NativeDisplayCommand::SvgPolygonFill {
                    node_id,
                    rect: bounds,
                    points: points.clone(),
                    color,
                    clip,
                });
            }
        } else if shape == "path" {
            if let Some(subpaths) = subpaths.as_ref() {
                commands.push(NativeDisplayCommand::SvgPathFill {
                    node_id,
                    rect: bounds,
                    subpaths: subpaths.clone(),
                    color,
                    clip,
                });
            }
        }
    }

    let Some(stroke) = svg_presentation_value(node, "stroke") else {
        return commands;
    };
    if stroke.eq_ignore_ascii_case("none") {
        return commands;
    }
    let Some(color) = svg_paint_color(document, node_id, stroke) else {
        return commands;
    };
    let width = svg_stroke_width(node);
    if width == 0 {
        return commands;
    }
    if shape == "path" {
        if let Some(subpaths) = subpaths {
            commands.push(NativeDisplayCommand::SvgPathStroke {
                node_id,
                rect: bounds,
                subpaths,
                width,
                color,
                clip,
            });
        }
    } else if matches!(shape, "line" | "polyline" | "polygon") {
        if let Some(points) = points {
            commands.push(NativeDisplayCommand::SvgPolyline {
                node_id,
                rect: bounds,
                points,
                closed: shape == "polygon",
                width,
                color,
                clip,
            });
        }
    } else {
        commands.push(NativeDisplayCommand::SvgStroke {
            node_id,
            shape: if shape == "rect" {
                NativeSvgStrokeShape::Rect
            } else {
                NativeSvgStrokeShape::Ellipse
            },
            rect: bounds,
            width,
            color,
            clip,
        });
    }
    commands
}

fn svg_paint_color(
    document: &NativeDocument,
    node_id: NativeNodeId,
    value: &str,
) -> Option<NativeColor> {
    if value.eq_ignore_ascii_case("currentcolor") {
        return document.computed_style_for_layout(node_id).color();
    }
    super::css::parse_color(value)
}

fn svg_stroke_width(node: &NativeNode) -> u32 {
    let Some(value) = svg_presentation_value(node, "stroke-width") else {
        return 1;
    };
    let value = value.trim();
    let value = value.strip_suffix("px").map(str::trim).unwrap_or(value);
    let Ok(value) = value.parse::<f64>() else {
        return 1;
    };
    if !value.is_finite() || value < 0.0 {
        return 1;
    }
    value.ceil().min(f64::from(MAX_NATIVE_SVG_STROKE_WIDTH)) as u32
}

fn svg_presentation_value<'a>(node: &'a NativeNode, property: &str) -> Option<&'a str> {
    if let Some(value) = node.attribute(property) {
        return Some(value);
    }
    let style = node.attribute("style")?;
    style.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case(property)
            .then_some(value.trim())
    })
}

fn paint_clip(
    document: &NativeDocument,
    layout: &NativeLayoutSnapshot,
    id: NativeNodeId,
) -> Option<NativeRect> {
    layout.overflow_clip_for(document, id)
}

fn push_command(
    commands: &mut Vec<NativeDisplayCommand>,
    command: NativeDisplayCommand,
) -> Result<(), NativeEngineError> {
    if commands.len() >= MAX_NATIVE_DISPLAY_COMMANDS {
        return Err(NativeEngineError::limit(
            "native display commands",
            MAX_NATIVE_DISPLAY_COMMANDS,
            commands.len().saturating_add(1),
        ));
    }
    commands.push(command);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::native_engine::{NativeEngineLimits, Viewport};

    #[test]
    fn display_list_rejects_stale_layout_revision() {
        let mut document =
            NativeDocument::parse("<main><p>Paint</p></main>", &NativeEngineLimits::default())
                .unwrap();
        let layout = document
            .layout(Viewport {
                width: 320,
                height: 200,
                device_scale_factor_milli: 1000,
            })
            .unwrap();
        document.set_revision(2);

        assert!(matches!(
            NativeDisplayList::build(&document, &layout),
            Err(NativeEngineError::InvalidConfiguration { field, .. })
                if field == "layout snapshot"
        ));
    }
}
