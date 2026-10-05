use super::config::MAX_NATIVE_NODES;
use super::css::{
    FontStyleValue, NATIVE_BACKGROUND_PERCENT_SCALE, NativeBackgroundPosition,
    NativeBackgroundPositionComponent, NativeBackgroundRepeat, NativeBackgroundSize,
    NativeBackgroundSizeComponent, NativeBorderRadius, NativeBorderStyle, NativeColor,
    NativeTextDecorationSkipInk, NativeTextDecorationSkipSpaces, NativeTextDecorationStyle,
};
use super::dom::{NativeDocument, NativeNode, NativeNodeId};
use super::error::NativeEngineError;
use super::font::NativeFontRun;
use super::image::decode_data_image;
use super::layout::{
    MAX_NATIVE_SVG_POINTS, NativeLayoutPaintOrder, NativeLayoutSnapshot, NativePoint, NativeRect,
    NativeSvgSubpath, svg_image_viewport, svg_line_points, svg_path_subpaths, svg_points,
    svg_preserve_aspect_ratio, svg_transform_for_node, svg_transform_points,
    svg_transformed_points, svg_transformed_subpaths,
};
use std::sync::Arc;

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
    SetNestedScrollOffset {
        offset: NativePoint,
    },
    FillRect {
        node_id: NativeNodeId,
        rect: NativeRect,
        radius: NativeBorderRadius,
        color: NativeColor,
        clip: Option<NativeRect>,
    },
    Image {
        node_id: NativeNodeId,
        rect: NativeRect,
        source_rect: NativeRect,
        source_width: u32,
        source_height: u32,
        pixels: Arc<[u8]>,
        clip: Option<NativeRect>,
    },
    SvgImage {
        node_id: NativeNodeId,
        rect: NativeRect,
        points: [NativePoint; 4],
        source_rect: NativeRect,
        source_width: u32,
        source_height: u32,
        pixels: Arc<[u8]>,
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
    GlyphRun {
        node_id: NativeNodeId,
        origin: NativePoint,
        text: String,
        truncated: bool,
        run: NativeFontRun,
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
        let ordered_paint_order = paint_order_indices(layout);
        let mut nested_scroll_offset = NativePoint { x: 0, y: 0 };
        for order in 0..layout.paint_order.len() {
            let entry_index = ordered_paint_order
                .as_ref()
                .map_or(order, |indices| indices[order]);
            let next_nested_scroll_offset = match layout.paint_order[entry_index] {
                NativeLayoutPaintOrder::Box(box_index) => layout
                    .boxes
                    .get(box_index)
                    .map(|layout_box| {
                        layout.nested_scroll_offset_for(document, layout_box.node_id, false)
                    })
                    .unwrap_or(NativePoint { x: 0, y: 0 }),
                NativeLayoutPaintOrder::Text(text_index) => layout
                    .text_runs
                    .get(text_index)
                    .map(|text_run| {
                        layout.nested_scroll_offset_for(document, text_run.node_id, true)
                    })
                    .unwrap_or(NativePoint { x: 0, y: 0 }),
                NativeLayoutPaintOrder::BeginOpacityGroup { .. }
                | NativeLayoutPaintOrder::EndOpacityGroup { .. } => nested_scroll_offset,
            };
            if next_nested_scroll_offset != nested_scroll_offset {
                push_command(
                    &mut commands,
                    NativeDisplayCommand::SetNestedScrollOffset {
                        offset: next_nested_scroll_offset,
                    },
                )?;
                nested_scroll_offset = next_nested_scroll_offset;
            }
            match layout.paint_order[entry_index] {
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
                    let rect = layout_box.rect;
                    let clip = paint_clip(document, layout, layout_box.node_id);
                    if let Some(color) = style.background_color() {
                        push_command(
                            &mut commands,
                            NativeDisplayCommand::FillRect {
                                node_id: layout_box.node_id,
                                rect,
                                radius: style.border_radius(),
                                color,
                                clip,
                            },
                        )?;
                    }
                    for command in
                        background_image_paint_commands(document, layout_box.node_id, rect, clip)?
                    {
                        push_command(&mut commands, command)?;
                    }
                    if let Some(border) = style.border()
                        && border.any_width()
                    {
                        let borders = NativeBorderPaint::from_style(border);
                        push_command(
                            &mut commands,
                            NativeDisplayCommand::BorderRect {
                                node_id: layout_box.node_id,
                                rect,
                                radius: style.border_radius(),
                                borders,
                                clip,
                            },
                        )?;
                    }
                    if let Some(command) =
                        image_paint_command(document, layout_box.node_id, rect, clip)
                    {
                        push_command(&mut commands, command)?;
                    }
                    if let Some(command) =
                        canvas_paint_command(document, layout_box.node_id, rect, clip)
                    {
                        push_command(&mut commands, command)?;
                    }
                    for command in svg_paint_commands(document, layout_box.node_id, rect, clip) {
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
                    let font_run = document
                        .text_metrics_for_layout(text_run.node_id)
                        .rasterize(
                            &text_run.text,
                            style.letter_spacing(),
                            style.word_spacing(),
                            text_run.justify_spacing,
                        );
                    let bold = style.font_weight().is_bold();
                    let italic = style.font_style() == FontStyleValue::Italic;
                    let decoration_style = style.text_decoration_style();
                    let decoration_skip_ink = style.text_decoration_skip_ink();
                    let decoration_skip_spaces = style.text_decoration_skip_spaces();
                    let decoration_thickness = style.text_decoration_thickness();
                    let underline_offset = style.text_underline_offset();
                    let underline = decoration.underline();
                    let overline = decoration.overline();
                    let line_through = decoration.line_through();
                    let word_spacing = style.word_spacing();
                    let letter_spacing = style.letter_spacing();
                    let justify_spacing = text_run.justify_spacing;
                    let command = if let Some(run) = font_run {
                        NativeDisplayCommand::GlyphRun {
                            node_id: text_run.node_id,
                            origin: text_run.origin,
                            text: text_run.text.clone(),
                            truncated: text_run.truncated,
                            run,
                            color,
                            decoration_color,
                            decoration_style,
                            decoration_skip_ink,
                            decoration_skip_spaces,
                            decoration_thickness,
                            underline_offset,
                            underline,
                            overline,
                            line_through,
                            bold,
                            italic,
                            word_spacing,
                            letter_spacing,
                            justify_spacing,
                            clip,
                        }
                    } else {
                        NativeDisplayCommand::TextRun {
                            node_id: text_run.node_id,
                            origin: text_run.origin,
                            text: text_run.text.clone(),
                            truncated: text_run.truncated,
                            color,
                            decoration_color,
                            decoration_style,
                            decoration_skip_ink,
                            decoration_skip_spaces,
                            decoration_thickness,
                            underline_offset,
                            underline,
                            overline,
                            line_through,
                            bold,
                            italic,
                            word_spacing,
                            letter_spacing,
                            justify_spacing,
                            clip,
                        }
                    };
                    push_command(&mut commands, command)?;
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

fn paint_order_indices(layout: &NativeLayoutSnapshot) -> Option<Vec<usize>> {
    let has_non_default_z_index = layout
        .boxes
        .iter()
        .any(|layout_box| layout_box.z_index != 0)
        || layout
            .text_runs
            .iter()
            .any(|text_run| text_run.z_index != 0);
    if !has_non_default_z_index {
        return None;
    }

    let mut z_indices = vec![0; layout.paint_order.len()];
    let mut opacity_depth = 0usize;
    let mut outermost_opacity_z_index = None;
    for (order, entry) in layout.paint_order.iter().copied().enumerate() {
        let entry_z_index = paint_order_entry_z_index(layout, entry);
        match entry {
            NativeLayoutPaintOrder::BeginOpacityGroup { .. } => {
                if opacity_depth == 0 {
                    outermost_opacity_z_index = Some(entry_z_index);
                }
                opacity_depth = opacity_depth.saturating_add(1);
                z_indices[order] = outermost_opacity_z_index.unwrap_or(entry_z_index);
            }
            NativeLayoutPaintOrder::EndOpacityGroup { .. } => {
                z_indices[order] = outermost_opacity_z_index.unwrap_or(entry_z_index);
                opacity_depth = opacity_depth.saturating_sub(1);
                if opacity_depth == 0 {
                    outermost_opacity_z_index = None;
                }
            }
            NativeLayoutPaintOrder::Box(_) | NativeLayoutPaintOrder::Text(_) => {
                z_indices[order] = outermost_opacity_z_index.unwrap_or(entry_z_index);
            }
        }
    }

    let mut indices = (0..layout.paint_order.len()).collect::<Vec<_>>();
    indices.sort_by_key(|order| (z_indices[*order], *order));
    Some(indices)
}

fn paint_order_entry_z_index(layout: &NativeLayoutSnapshot, entry: NativeLayoutPaintOrder) -> i32 {
    match entry {
        NativeLayoutPaintOrder::Box(index) => layout
            .boxes
            .get(index)
            .map_or(0, |layout_box| layout_box.z_index),
        NativeLayoutPaintOrder::Text(index) => layout
            .text_runs
            .get(index)
            .map_or(0, |text_run| text_run.z_index),
        NativeLayoutPaintOrder::BeginOpacityGroup { node_id, .. }
        | NativeLayoutPaintOrder::EndOpacityGroup { node_id } => layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .map_or(0, |layout_box| layout_box.z_index),
    }
}

fn image_paint_command(
    document: &NativeDocument,
    node_id: NativeNodeId,
    bounds: NativeRect,
    clip: Option<NativeRect>,
) -> Option<NativeDisplayCommand> {
    let node = document.node(node_id)?;
    if node.element_name() != Some("img") || bounds.width == 0 || bounds.height == 0 {
        return None;
    }
    let image = document
        .image_resource_for_node(node_id)
        .cloned()
        .or_else(|| decode_data_image(node.attribute("src")?))?;
    Some(NativeDisplayCommand::Image {
        node_id,
        rect: bounds,
        source_rect: NativeRect {
            x: 0,
            y: 0,
            width: image.width,
            height: image.height,
        },
        source_width: image.width,
        source_height: image.height,
        pixels: Arc::from(image.current_pixels()),
        clip,
    })
}

fn canvas_paint_command(
    document: &NativeDocument,
    node_id: NativeNodeId,
    bounds: NativeRect,
    clip: Option<NativeRect>,
) -> Option<NativeDisplayCommand> {
    let node = document.node(node_id)?;
    if node.element_name() != Some("canvas") || bounds.width == 0 || bounds.height == 0 {
        return None;
    }
    let canvas = document.canvas_resource_for_node(node_id)?;
    Some(NativeDisplayCommand::Image {
        node_id,
        rect: bounds,
        source_rect: NativeRect {
            x: 0,
            y: 0,
            width: canvas.width,
            height: canvas.height,
        },
        source_width: canvas.width,
        source_height: canvas.height,
        pixels: Arc::from(canvas.pixels.as_slice()),
        clip,
    })
}

fn background_image_paint_commands(
    document: &NativeDocument,
    node_id: NativeNodeId,
    bounds: NativeRect,
    clip: Option<NativeRect>,
) -> Result<Vec<NativeDisplayCommand>, NativeEngineError> {
    if bounds.width == 0 || bounds.height == 0 {
        return Ok(Vec::new());
    }
    let Some(source) = document.background_image_source_for_node(node_id) else {
        return Ok(Vec::new());
    };
    let image = document
        .background_image_resource_for_node(node_id)
        .cloned()
        .or_else(|| decode_data_image(source));
    let Some(image) = image else {
        return Ok(Vec::new());
    };
    let pixels = Arc::<[u8]>::from(image.current_pixels());
    let style = document.computed_style_for_layout(node_id);
    let Some((tile_width, tile_height)) =
        background_tile_size(image.width, image.height, bounds, style.background_size())
    else {
        return Ok(Vec::new());
    };
    let position = background_position_origin(
        bounds,
        (tile_width, tile_height),
        style.background_position(),
    );
    let (repeat_x, repeat_y) = match style.background_repeat() {
        NativeBackgroundRepeat::Repeat => (true, true),
        NativeBackgroundRepeat::RepeatX => (true, false),
        NativeBackgroundRepeat::RepeatY => (false, true),
        NativeBackgroundRepeat::NoRepeat => (false, false),
        NativeBackgroundRepeat::CustomProperty(_)
        | NativeBackgroundRepeat::CustomPropertyFallback(_, _)
        | NativeBackgroundRepeat::CustomPropertyShorthand(_)
        | NativeBackgroundRepeat::CustomPropertyShorthandFallback(_, _) => (true, true),
    };
    let x_starts = background_tile_starts(
        i64::from(bounds.x),
        i64::from(bounds.right()),
        position.0,
        i64::from(tile_width),
        repeat_x,
    );
    let y_starts = background_tile_starts(
        i64::from(bounds.y),
        i64::from(bounds.bottom()),
        position.1,
        i64::from(tile_height),
        repeat_y,
    );
    let tile_count = x_starts.len().saturating_mul(y_starts.len());
    if tile_count > MAX_NATIVE_BACKGROUND_TILES {
        return Err(NativeEngineError::limit(
            "native background tiles",
            MAX_NATIVE_BACKGROUND_TILES,
            tile_count,
        ));
    }
    let mut commands = Vec::new();
    for tile_y in y_starts {
        for tile_x in &x_starts {
            let Some(tile_right) = tile_x.checked_add(i64::from(tile_width)) else {
                continue;
            };
            let Some(tile_bottom) = tile_y.checked_add(i64::from(tile_height)) else {
                continue;
            };
            let visible_left = (*tile_x).max(i64::from(bounds.x));
            let visible_top = tile_y.max(i64::from(bounds.y));
            let visible_right = tile_right.min(i64::from(bounds.right()));
            let visible_bottom = tile_bottom.min(i64::from(bounds.bottom()));
            if visible_left >= visible_right || visible_top >= visible_bottom {
                continue;
            }
            let Some(rect) = signed_rect(visible_left, visible_top, visible_right, visible_bottom)
            else {
                continue;
            };
            commands.push(NativeDisplayCommand::Image {
                node_id,
                rect,
                source_rect: background_source_rect(
                    *tile_x,
                    tile_y,
                    tile_right,
                    tile_bottom,
                    visible_left,
                    visible_top,
                    visible_right,
                    visible_bottom,
                    image.width,
                    image.height,
                ),
                source_width: image.width,
                source_height: image.height,
                pixels: Arc::clone(&pixels),
                clip: clip.and_then(|clip| intersect_rects(Some(bounds), Some(clip))),
            });
        }
    }
    Ok(commands)
}

const MAX_NATIVE_BACKGROUND_TILES: usize = MAX_NATIVE_DISPLAY_COMMANDS;

fn signed_rect(left: i64, top: i64, right: i64, bottom: i64) -> Option<NativeRect> {
    if left < 0 || top < 0 || left >= right || top >= bottom {
        return None;
    }
    Some(NativeRect {
        x: u32::try_from(left).ok()?,
        y: u32::try_from(top).ok()?,
        width: u32::try_from(right.saturating_sub(left)).ok()?,
        height: u32::try_from(bottom.saturating_sub(top)).ok()?,
    })
}

fn intersect_rects(first: Option<NativeRect>, second: Option<NativeRect>) -> Option<NativeRect> {
    match (first, second) {
        (Some(first), Some(second)) => signed_rect(
            i64::from(first.x.max(second.x)),
            i64::from(first.y.max(second.y)),
            i64::from(first.right().min(second.right())),
            i64::from(first.bottom().min(second.bottom())),
        ),
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (None, None) => None,
    }
}

fn background_size_component_pixels(
    component: NativeBackgroundSizeComponent,
    available: u32,
) -> Option<Option<u32>> {
    match component {
        NativeBackgroundSizeComponent::Auto => Some(None),
        NativeBackgroundSizeComponent::Length(value) => Some(Some(value)),
        NativeBackgroundSizeComponent::Percentage(value) => {
            let pixels = u64::from(available)
                .saturating_mul(u64::from(value))
                .checked_add(u64::from(NATIVE_BACKGROUND_PERCENT_SCALE as u32 / 2))?
                / u64::from(NATIVE_BACKGROUND_PERCENT_SCALE as u32);
            Some(Some(u32::try_from(pixels).ok()?))
        }
    }
}

fn scaled_background_dimension(value: u32, numerator: u32, denominator: u32) -> Option<u32> {
    if value == 0 || denominator == 0 {
        return None;
    }
    let scaled = u64::from(value)
        .saturating_mul(u64::from(numerator))
        .checked_add(u64::from(denominator / 2))?
        / u64::from(denominator);
    u32::try_from(scaled).ok()
}

fn background_tile_size(
    image_width: u32,
    image_height: u32,
    bounds: NativeRect,
    size: NativeBackgroundSize,
) -> Option<(u32, u32)> {
    if image_width == 0 || image_height == 0 {
        return None;
    }
    let (width, height) = match size {
        NativeBackgroundSize::Explicit { width, height } => {
            let width = background_size_component_pixels(width, bounds.width)?;
            let height = background_size_component_pixels(height, bounds.height)?;
            match (width, height) {
                (Some(width), Some(height)) => (width, height),
                (Some(width), None) => (
                    width,
                    scaled_background_dimension(image_height, width, image_width)?,
                ),
                (None, Some(height)) => (
                    scaled_background_dimension(image_width, height, image_height)?,
                    height,
                ),
                (None, None) => (image_width, image_height),
            }
        }
        NativeBackgroundSize::Cover => {
            if u64::from(bounds.width).saturating_mul(u64::from(image_height))
                >= u64::from(bounds.height).saturating_mul(u64::from(image_width))
            {
                (
                    bounds.width,
                    scaled_background_dimension(image_height, bounds.width, image_width)?,
                )
            } else {
                (
                    scaled_background_dimension(image_width, bounds.height, image_height)?,
                    bounds.height,
                )
            }
        }
        NativeBackgroundSize::Contain => {
            if u64::from(bounds.width).saturating_mul(u64::from(image_height))
                <= u64::from(bounds.height).saturating_mul(u64::from(image_width))
            {
                (
                    bounds.width,
                    scaled_background_dimension(image_height, bounds.width, image_width)?,
                )
            } else {
                (
                    scaled_background_dimension(image_width, bounds.height, image_height)?,
                    bounds.height,
                )
            }
        }
    };
    (width > 0 && height > 0).then_some((width, height))
}

fn background_position_offset(
    start: u32,
    available: u32,
    tile: u32,
    component: NativeBackgroundPositionComponent,
) -> i64 {
    let available = i64::from(available).saturating_sub(i64::from(tile));
    let offset = match component {
        NativeBackgroundPositionComponent::Length(value) => i64::from(value),
        NativeBackgroundPositionComponent::Percentage(value) => available
            .saturating_mul(i64::from(value))
            .checked_div(i64::from(NATIVE_BACKGROUND_PERCENT_SCALE))
            .unwrap_or_default(),
    };
    i64::from(start).saturating_add(offset)
}

fn background_position_origin(
    bounds: NativeRect,
    tile: (u32, u32),
    position: NativeBackgroundPosition,
) -> (i64, i64) {
    (
        background_position_offset(bounds.x, bounds.width, tile.0, position.x),
        background_position_offset(bounds.y, bounds.height, tile.1, position.y),
    )
}

fn background_tile_starts(
    range_start: i64,
    range_end: i64,
    origin: i64,
    tile: i64,
    repeat: bool,
) -> Vec<i64> {
    if tile <= 0 || range_start >= range_end {
        return Vec::new();
    }
    if !repeat {
        return vec![origin];
    }
    let offset = range_start.saturating_sub(origin).div_euclid(tile);
    let Some(mut start) = origin.checked_add(offset.saturating_mul(tile)) else {
        return Vec::new();
    };
    let mut starts = Vec::new();
    while start < range_end && starts.len() <= MAX_NATIVE_BACKGROUND_TILES {
        starts.push(start);
        let Some(next) = start.checked_add(tile) else {
            break;
        };
        start = next;
    }
    starts
}

fn background_source_range(
    tile_start: i64,
    tile_end: i64,
    visible_start: i64,
    visible_end: i64,
    source_length: u32,
) -> (u32, u32) {
    let tile_length = u64::try_from(tile_end.saturating_sub(tile_start)).unwrap_or(1);
    let start_offset = u64::try_from(visible_start.saturating_sub(tile_start)).unwrap_or_default();
    let end_offset = u64::try_from(visible_end.saturating_sub(tile_start)).unwrap_or_default();
    let source_length = u64::from(source_length);
    let source_start = start_offset
        .saturating_mul(source_length)
        .checked_div(tile_length)
        .unwrap_or_default()
        .min(source_length.saturating_sub(1));
    let source_end = end_offset
        .saturating_mul(source_length)
        .saturating_add(tile_length.saturating_sub(1))
        .checked_div(tile_length)
        .unwrap_or(source_length)
        .min(source_length)
        .max(source_start.saturating_add(1).min(source_length));
    (
        u32::try_from(source_start).unwrap_or_default(),
        u32::try_from(source_end.saturating_sub(source_start)).unwrap_or(1),
    )
}

#[allow(clippy::too_many_arguments)]
fn background_source_rect(
    tile_left: i64,
    tile_top: i64,
    tile_right: i64,
    tile_bottom: i64,
    visible_left: i64,
    visible_top: i64,
    visible_right: i64,
    visible_bottom: i64,
    source_width: u32,
    source_height: u32,
) -> NativeRect {
    let (x, width) = background_source_range(
        tile_left,
        tile_right,
        visible_left,
        visible_right,
        source_width,
    );
    let (y, height) = background_source_range(
        tile_top,
        tile_bottom,
        visible_top,
        visible_bottom,
        source_height,
    );
    NativeRect {
        x,
        y,
        width,
        height,
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
    if shape == "image" {
        return svg_image_paint_command(document, node_id, clip)
            .into_iter()
            .collect();
    }
    if !matches!(
        shape,
        "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path"
    ) || bounds.width == 0
        || bounds.height == 0
    {
        return Vec::new();
    }
    let Some(transform) = svg_transform_for_node(document, node_id) else {
        return Vec::new();
    };
    let transformed = !transform.is_identity();
    let points = match shape {
        "line" => {
            if transformed {
                svg_transformed_points(node, transform)
            } else {
                Some(svg_line_points(node))
            }
        }
        "polyline" | "polygon" => {
            if transformed {
                svg_transformed_points(node, transform)
            } else {
                svg_points(node)
            }
        }
        _ => None,
    };
    let transformed_shape_points = (transformed && matches!(shape, "rect" | "circle" | "ellipse"))
        .then(|| svg_transformed_points(node, transform))
        .flatten();
    let subpaths = if shape == "path" {
        if transformed {
            svg_transformed_subpaths(node, transform)
        } else {
            svg_path_subpaths(node)
        }
    } else {
        None
    };
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
            if let Some(points) = transformed_shape_points.as_ref() {
                commands.push(NativeDisplayCommand::SvgPolygonFill {
                    node_id,
                    rect: bounds,
                    points: points.clone(),
                    color,
                    clip,
                });
            } else {
                commands.push(NativeDisplayCommand::FillRect {
                    node_id,
                    rect: bounds,
                    radius: NativeBorderRadius::default(),
                    color,
                    clip,
                });
            }
        } else if matches!(shape, "circle" | "ellipse") {
            if let Some(points) = transformed_shape_points.as_ref() {
                commands.push(NativeDisplayCommand::SvgPolygonFill {
                    node_id,
                    rect: bounds,
                    points: points.clone(),
                    color,
                    clip,
                });
            } else {
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
        } else if shape == "path"
            && let Some(subpaths) = subpaths.as_ref()
        {
            commands.push(NativeDisplayCommand::SvgPathFill {
                node_id,
                rect: bounds,
                subpaths: subpaths.clone(),
                color,
                clip,
            });
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
    } else if let Some(points) = transformed_shape_points {
        commands.push(NativeDisplayCommand::SvgPolyline {
            node_id,
            rect: bounds,
            points,
            closed: true,
            width,
            color,
            clip,
        });
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

fn svg_image_paint_command(
    document: &NativeDocument,
    node_id: NativeNodeId,
    clip: Option<NativeRect>,
) -> Option<NativeDisplayCommand> {
    if !document.is_svg_image_node(node_id) {
        return None;
    }
    let node = document.node(node_id)?;
    let image = document
        .image_resource_for_node(node_id)
        .cloned()
        .or_else(|| {
            document
                .svg_image_source_for_node(node_id)
                .and_then(decode_data_image)
        })?;
    let (x, y, width, height) = svg_image_viewport(node)?;
    let transform = svg_transform_for_node(document, node_id)?;
    let (align_x, align_y, slice) = svg_preserve_aspect_ratio(node)?;
    let mut destination = (x, y, width, height);
    let mut source_rect = NativeRect {
        x: 0,
        y: 0,
        width: image.width,
        height: image.height,
    };

    if let (Some(align_x), Some(align_y)) = (align_x, align_y) {
        let scale_x = width / f64::from(image.width);
        let scale_y = height / f64::from(image.height);
        let scale = if slice {
            scale_x.max(scale_y)
        } else {
            scale_x.min(scale_y)
        };
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        if slice {
            let source_width = (width / scale).clamp(1.0, f64::from(image.width));
            let source_height = (height / scale).clamp(1.0, f64::from(image.height));
            let source_x = align_x * (f64::from(image.width) - source_width);
            let source_y = align_y * (f64::from(image.height) - source_height);
            let left = source_x.floor().clamp(0.0, f64::from(image.width - 1)) as u32;
            let top = source_y.floor().clamp(0.0, f64::from(image.height - 1)) as u32;
            let right = (source_x + source_width)
                .ceil()
                .clamp(f64::from(left + 1), f64::from(image.width)) as u32;
            let bottom = (source_y + source_height)
                .ceil()
                .clamp(f64::from(top + 1), f64::from(image.height)) as u32;
            source_rect = NativeRect {
                x: left,
                y: top,
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            };
        } else {
            let rendered_width = f64::from(image.width) * scale;
            let rendered_height = f64::from(image.height) * scale;
            destination = (
                x + (width - rendered_width) * align_x,
                y + (height - rendered_height) * align_y,
                rendered_width,
                rendered_height,
            );
        }
    }

    let (x, y, width, height) = destination;
    let points = svg_transform_points(
        &[
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
        ],
        transform,
    )?;
    let points: [NativePoint; 4] = points.try_into().ok()?;
    let min_x = points.iter().map(|point| point.x).min()?;
    let min_y = points.iter().map(|point| point.y).min()?;
    let max_x = points.iter().map(|point| point.x).max()?;
    let max_y = points.iter().map(|point| point.y).max()?;
    let rect = NativeRect {
        x: min_x,
        y: min_y,
        width: max_x.saturating_sub(min_x).saturating_add(1),
        height: max_y.saturating_sub(min_y).saturating_add(1),
    };
    Some(NativeDisplayCommand::SvgImage {
        node_id,
        rect,
        points,
        source_rect,
        source_width: image.width,
        source_height: image.height,
        pixels: Arc::from(image.current_pixels()),
        clip,
    })
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
