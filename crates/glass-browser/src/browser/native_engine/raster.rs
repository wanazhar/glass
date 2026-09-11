use super::css::{
    NativeBorderRadius, NativeBorderStyle, NativeTextDecorationSkipInk,
    NativeTextDecorationSkipSpaces, NativeTextDecorationStyle,
};
use super::error::NativeEngineError;
use super::layout::{NativePoint, NativeRect, NativeSvgSubpath, rounded_rect_contains};
use super::paint::{
    MAX_NATIVE_DISPLAY_COMMANDS, NativeDisplayCommand, NativeDisplayList, NativeSvgStrokeShape,
    NativeTextLineBoundary,
};

/// Maximum number of logical pixels retained by one native software surface.
pub const MAX_NATIVE_SURFACE_PIXELS: usize = 4 * 1024 * 1024;
/// Maximum nesting depth for bounded opacity groups.
pub const MAX_NATIVE_OPACITY_GROUP_DEPTH: usize = 8;
/// Maximum logical pixels retained across the root surface and opacity layers.
pub const MAX_NATIVE_OPACITY_LAYER_PIXELS: usize = MAX_NATIVE_SURFACE_PIXELS * 4;
const GLYPH_WIDTH: u32 = 5;
const GLYPH_ADVANCE: u32 = 6;
const GLYPH_HEIGHT: u32 = 7;
const ITALIC_ROW_SHIFTS: [u32; GLYPH_HEIGHT as usize] = [2, 2, 1, 1, 1, 0, 0];
const WAVY_DECORATION_PHASE: [i64; 8] = [0, 1, 2, 1, 0, -1, -2, -1];

#[derive(Debug, Clone, Copy)]
struct TextPaint {
    color: super::css::NativeColor,
    decoration_color: super::css::NativeColor,
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
}

#[derive(Debug, Clone, Copy)]
enum NativeBorderEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Immutable logical RGBA output from the native display-list seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSurface {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl NativeSurface {
    /// Replay one bounded display list into a logical-pixel surface.
    pub fn from_display_list(display_list: &NativeDisplayList) -> Result<Self, NativeEngineError> {
        if display_list.commands.len() > MAX_NATIVE_DISPLAY_COMMANDS {
            return Err(NativeEngineError::limit(
                "native display commands",
                MAX_NATIVE_DISPLAY_COMMANDS,
                display_list.commands.len(),
            ));
        }
        display_list.viewport.validate()?;
        let width = display_list.viewport.width;
        let height = display_list.viewport.height;
        let pixels = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| {
                NativeEngineError::invalid("surface pixels", "pixel count exceeds host bounds")
            })?;
        if pixels > MAX_NATIVE_SURFACE_PIXELS {
            return Err(NativeEngineError::limit(
                "native surface pixels",
                MAX_NATIVE_SURFACE_PIXELS,
                pixels,
            ));
        }
        let byte_len = pixels.checked_mul(4).ok_or_else(|| {
            NativeEngineError::invalid("surface bytes", "RGBA byte count exceeds host bounds")
        })?;
        let mut surfaces = vec![Self {
            width,
            height,
            rgba: vec![0; byte_len],
        }];
        let mut opacity_groups = Vec::new();
        let scroll_offset = display_list.scroll_offset;
        let mut text_run_index = 0usize;
        for command in &display_list.commands {
            match command {
                NativeDisplayCommand::BeginOpacityGroup { node_id, opacity } => {
                    if opacity_groups.len() >= MAX_NATIVE_OPACITY_GROUP_DEPTH {
                        return Err(NativeEngineError::limit(
                            "native opacity group depth",
                            MAX_NATIVE_OPACITY_GROUP_DEPTH,
                            opacity_groups.len().saturating_add(1),
                        ));
                    }
                    let layer_count = surfaces.len().checked_add(1).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "native opacity layers",
                            "layer count exceeds host bounds",
                        )
                    })?;
                    let total_pixels = layer_count.checked_mul(pixels).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "native opacity layer pixels",
                            "aggregate pixel count exceeds host bounds",
                        )
                    })?;
                    if total_pixels > MAX_NATIVE_OPACITY_LAYER_PIXELS {
                        return Err(NativeEngineError::limit(
                            "native opacity layer pixels",
                            MAX_NATIVE_OPACITY_LAYER_PIXELS,
                            total_pixels,
                        ));
                    }
                    surfaces.push(Self {
                        width,
                        height,
                        rgba: vec![0; byte_len],
                    });
                    opacity_groups.push((*node_id, *opacity));
                }
                NativeDisplayCommand::Clear { color } => {
                    Self::current_surface_mut(&mut surfaces)?.clear(*color);
                }
                NativeDisplayCommand::FillRect {
                    rect,
                    radius,
                    color,
                    clip,
                    ..
                } => {
                    let Some(viewport_rect) = Self::translate_rect(*rect, scroll_offset) else {
                        continue;
                    };
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.fill_rect(
                        viewport_rect,
                        *rect,
                        *radius,
                        *color,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::Image {
                    rect,
                    source_width,
                    source_height,
                    pixels,
                    clip,
                    ..
                } => {
                    let Some(viewport_rect) = Self::translate_rect(*rect, scroll_offset) else {
                        continue;
                    };
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.draw_image(
                        viewport_rect,
                        *rect,
                        *source_width,
                        *source_height,
                        pixels,
                        clip,
                        scroll_offset,
                    )?;
                }
                NativeDisplayCommand::SvgStroke {
                    shape,
                    rect,
                    width,
                    color,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.svg_stroke(
                        *rect,
                        *shape,
                        *width,
                        *color,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::SvgPolygonFill {
                    rect,
                    points,
                    color,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.svg_polygon_fill(
                        *rect,
                        points,
                        *color,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::SvgPolyline {
                    rect,
                    points,
                    closed,
                    width,
                    color,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.svg_polyline(
                        *rect,
                        points,
                        *closed,
                        *width,
                        *color,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::SvgPathFill {
                    rect,
                    subpaths,
                    color,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.svg_path_fill(
                        *rect,
                        subpaths,
                        *color,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::SvgPathStroke {
                    rect,
                    subpaths,
                    width,
                    color,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.svg_path_stroke(
                        *rect,
                        subpaths,
                        *width,
                        *color,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::BorderRect {
                    rect,
                    radius,
                    borders,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    Self::current_surface_mut(&mut surfaces)?.border_rect(
                        *rect,
                        *radius,
                        *borders,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::TextRun {
                    origin,
                    text,
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
                    ..
                } => {
                    let line_boundary = display_list
                        .text_run_boundaries
                        .get(text_run_index)
                        .copied()
                        .unwrap_or_default();
                    text_run_index = text_run_index.saturating_add(1);
                    if text.len() > crate::browser_backend::MAX_TEXT_BYTES {
                        return Err(NativeEngineError::limit(
                            "display text",
                            crate::browser_backend::MAX_TEXT_BYTES,
                            text.len(),
                        ));
                    }
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    let decoration_thickness = (*decoration_thickness)
                        .min(super::css::MAX_NATIVE_TEXT_DECORATION_THICKNESS);
                    let underline_offset = (*underline_offset).clamp(
                        super::css::MIN_NATIVE_TEXT_UNDERLINE_OFFSET,
                        super::css::MAX_NATIVE_TEXT_UNDERLINE_OFFSET,
                    );
                    Self::current_surface_mut(&mut surfaces)?.draw_text(
                        *origin,
                        text,
                        TextPaint {
                            color: *color,
                            decoration_color: *decoration_color,
                            decoration_style: *decoration_style,
                            decoration_skip_ink: *decoration_skip_ink,
                            decoration_skip_spaces: *decoration_skip_spaces,
                            decoration_thickness,
                            underline_offset,
                            underline: *underline,
                            overline: *overline,
                            line_through: *line_through,
                            bold: *bold,
                            italic: *italic,
                            word_spacing: *word_spacing,
                            letter_spacing: *letter_spacing,
                            justify_spacing: *justify_spacing,
                        },
                        line_boundary,
                        clip,
                        scroll_offset,
                    );
                }
                NativeDisplayCommand::EndOpacityGroup { node_id } => {
                    let Some((open_node_id, opacity)) = opacity_groups.pop() else {
                        return Err(NativeEngineError::invalid(
                            "native opacity group",
                            "end marker has no matching begin marker",
                        ));
                    };
                    if open_node_id != *node_id {
                        return Err(NativeEngineError::invalid(
                            "native opacity group",
                            "end marker does not match the open group",
                        ));
                    }
                    let layer = surfaces.pop().ok_or_else(|| {
                        NativeEngineError::invalid("native opacity group", "group layer is missing")
                    })?;
                    Self::current_surface_mut(&mut surfaces)?.composite_layer(&layer, opacity);
                }
            }
        }
        if !opacity_groups.is_empty() || surfaces.len() != 1 {
            return Err(NativeEngineError::invalid(
                "native opacity group",
                "display list has an unclosed group",
            ));
        }
        surfaces
            .pop()
            .ok_or_else(|| NativeEngineError::invalid("native surface", "root surface is missing"))
    }

    fn current_surface_mut(surfaces: &mut [Self]) -> Result<&mut Self, NativeEngineError> {
        surfaces.last_mut().ok_or_else(|| {
            NativeEngineError::invalid("native surface", "current surface is missing")
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    fn translate_rect(rect: NativeRect, scroll_offset: NativePoint) -> Option<NativeRect> {
        let left = rect.x.saturating_sub(scroll_offset.x);
        let top = rect.y.saturating_sub(scroll_offset.y);
        let right = rect.right().saturating_sub(scroll_offset.x);
        let bottom = rect.bottom().saturating_sub(scroll_offset.y);
        (left < right && top < bottom).then_some(NativeRect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        })
    }

    fn translate_clip(
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) -> Option<Option<NativeRect>> {
        match clip {
            Some(clip) => Self::translate_rect(clip, scroll_offset).map(Some),
            None => Some(None),
        }
    }

    /// Encode the logical surface as a bounded RGBA PNG payload.
    pub fn to_png(&self) -> Result<Vec<u8>, NativeEngineError> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| NativeEngineError::invalid("native PNG", error.to_string()))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|error| NativeEngineError::invalid("native PNG", error.to_string()))?;
        }
        if bytes.len() > crate::browser_backend::MAX_CAPTURE_BYTES {
            return Err(NativeEngineError::limit(
                "native PNG bytes",
                crate::browser_backend::MAX_CAPTURE_BYTES,
                bytes.len(),
            ));
        }
        Ok(bytes)
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = self.pixel_index(x, y)?;
        Some([
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ])
    }

    fn clear(&mut self, color: super::css::NativeColor) {
        for pixel in self.rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[color.red, color.green, color.blue, color.alpha]);
        }
    }

    fn fill_rect(
        &mut self,
        viewport_rect: NativeRect,
        document_rect: NativeRect,
        radius: NativeBorderRadius,
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        let Some((left, top, right, bottom)) = self.clipped_bounds(viewport_rect, clip) else {
            return;
        };
        for y in top..bottom {
            for x in left..right {
                let point = NativePoint {
                    x: x.saturating_add(scroll_offset.x),
                    y: y.saturating_add(scroll_offset.y),
                };
                if rounded_rect_contains(document_rect, radius, point) {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn draw_image(
        &mut self,
        viewport_rect: NativeRect,
        document_rect: NativeRect,
        source_width: u32,
        source_height: u32,
        pixels: &[u8],
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) -> Result<(), NativeEngineError> {
        if source_width == 0
            || source_height == 0
            || document_rect.width == 0
            || document_rect.height == 0
        {
            return Ok(());
        }
        let expected_len = usize::try_from(source_width)
            .ok()
            .and_then(|width| {
                usize::try_from(source_height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                NativeEngineError::invalid(
                    "native image",
                    "RGBA pixel payload dimensions exceed host bounds",
                )
            })?;
        if pixels.len() != expected_len {
            return Err(NativeEngineError::invalid(
                "native image",
                "RGBA pixel payload does not match dimensions",
            ));
        }
        let Some((left, top, right, bottom)) = self.clipped_bounds(viewport_rect, clip) else {
            return Ok(());
        };
        for y in top..bottom {
            let document_y = i64::from(y).saturating_add(i64::from(scroll_offset.y));
            let Some(local_y) = document_y
                .checked_sub(i64::from(document_rect.y))
                .and_then(|value| u64::try_from(value).ok())
            else {
                continue;
            };
            if local_y >= u64::from(document_rect.height) {
                continue;
            }
            let source_y = (local_y * u64::from(source_height) / u64::from(document_rect.height))
                .min(u64::from(source_height.saturating_sub(1)))
                as usize;
            for x in left..right {
                let document_x = i64::from(x).saturating_add(i64::from(scroll_offset.x));
                let Some(local_x) = document_x
                    .checked_sub(i64::from(document_rect.x))
                    .and_then(|value| u64::try_from(value).ok())
                else {
                    continue;
                };
                if local_x >= u64::from(document_rect.width) {
                    continue;
                }
                let source_x = (local_x * u64::from(source_width) / u64::from(document_rect.width))
                    .min(u64::from(source_width.saturating_sub(1)))
                    as usize;
                let index = (source_y * usize::try_from(source_width).unwrap_or(0) + source_x) * 4;
                self.blend_pixel(
                    x,
                    y,
                    super::css::NativeColor {
                        red: pixels[index],
                        green: pixels[index + 1],
                        blue: pixels[index + 2],
                        alpha: pixels[index + 3],
                    },
                );
            }
        }
        Ok(())
    }

    fn svg_stroke(
        &mut self,
        rect: NativeRect,
        shape: NativeSvgStrokeShape,
        width: u32,
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        if width == 0 || rect.width == 0 || rect.height == 0 {
            return;
        }
        let outer_left = i64::from(rect.x) - i64::from(scroll_offset.x);
        let outer_top = i64::from(rect.y) - i64::from(scroll_offset.y);
        let outer_right = i64::from(rect.right()) - i64::from(scroll_offset.x);
        let outer_bottom = i64::from(rect.bottom()) - i64::from(scroll_offset.y);
        let Some((left, top, right, bottom)) =
            self.clipped_signed_bounds(outer_left, outer_top, outer_right, outer_bottom, clip)
        else {
            return;
        };
        let inner_rect = NativeRect {
            x: rect.x.saturating_add(width),
            y: rect.y.saturating_add(width),
            width: rect.width.saturating_sub(width.saturating_mul(2)),
            height: rect.height.saturating_sub(width.saturating_mul(2)),
        };
        for y in top..bottom {
            for x in left..right {
                let document_x = i64::from(x).saturating_add(i64::from(scroll_offset.x));
                let document_y = i64::from(y).saturating_add(i64::from(scroll_offset.y));
                let paints = match shape {
                    NativeSvgStrokeShape::Rect => {
                        let Some(document_x) = u32::try_from(document_x).ok() else {
                            continue;
                        };
                        let Some(document_y) = u32::try_from(document_y).ok() else {
                            continue;
                        };
                        rect.contains(NativePoint {
                            x: document_x,
                            y: document_y,
                        }) && !inner_rect.contains(NativePoint {
                            x: document_x,
                            y: document_y,
                        })
                    }
                    NativeSvgStrokeShape::Ellipse => {
                        let pixel_x = document_x as f64 + 0.5;
                        let pixel_y = document_y as f64 + 0.5;
                        ellipse_contains(rect, pixel_x, pixel_y, 0.0)
                            && !ellipse_contains(rect, pixel_x, pixel_y, f64::from(width))
                    }
                };
                if paints {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn svg_polygon_fill(
        &mut self,
        rect: NativeRect,
        points: &[NativePoint],
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        if points.len() < 3 || rect.width == 0 || rect.height == 0 {
            return;
        }
        let outer_left = i64::from(rect.x) - i64::from(scroll_offset.x);
        let outer_top = i64::from(rect.y) - i64::from(scroll_offset.y);
        let outer_right = i64::from(rect.right()) - i64::from(scroll_offset.x);
        let outer_bottom = i64::from(rect.bottom()) - i64::from(scroll_offset.y);
        let Some((left, top, right, bottom)) =
            self.clipped_signed_bounds(outer_left, outer_top, outer_right, outer_bottom, clip)
        else {
            return;
        };
        for y in top..bottom {
            for x in left..right {
                let document_x = i64::from(x).saturating_add(i64::from(scroll_offset.x));
                let document_y = i64::from(y).saturating_add(i64::from(scroll_offset.y));
                if polygon_contains(points, document_x as f64 + 0.5, document_y as f64 + 0.5) {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn svg_polyline(
        &mut self,
        rect: NativeRect,
        points: &[NativePoint],
        closed: bool,
        width: u32,
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        if points.len() < 2 || width == 0 || rect.width == 0 || rect.height == 0 {
            return;
        }
        let padding = i64::from(width.saturating_add(1) / 2);
        let outer_left = i64::from(rect.x)
            .saturating_sub(padding)
            .saturating_sub(i64::from(scroll_offset.x));
        let outer_top = i64::from(rect.y)
            .saturating_sub(padding)
            .saturating_sub(i64::from(scroll_offset.y));
        let outer_right = i64::from(rect.right())
            .saturating_add(padding)
            .saturating_sub(i64::from(scroll_offset.x));
        let outer_bottom = i64::from(rect.bottom())
            .saturating_add(padding)
            .saturating_sub(i64::from(scroll_offset.y));
        let Some((left, top, right, bottom)) =
            self.clipped_signed_bounds(outer_left, outer_top, outer_right, outer_bottom, clip)
        else {
            return;
        };
        let radius_squared = f64::from(width) * f64::from(width) / 4.0;
        for y in top..bottom {
            for x in left..right {
                let document_x = i64::from(x).saturating_add(i64::from(scroll_offset.x));
                let document_y = i64::from(y).saturating_add(i64::from(scroll_offset.y));
                let pixel_x = document_x as f64 + 0.5;
                let pixel_y = document_y as f64 + 0.5;
                let mut paints = points.windows(2).any(|segment| {
                    distance_to_segment_squared(pixel_x, pixel_y, segment[0], segment[1])
                        <= radius_squared
                });
                if closed && !paints {
                    let first = points[0];
                    let last = points[points.len().saturating_sub(1)];
                    paints = distance_to_segment_squared(pixel_x, pixel_y, last, first)
                        <= radius_squared;
                }
                if paints {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn svg_path_fill(
        &mut self,
        rect: NativeRect,
        subpaths: &[NativeSvgSubpath],
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        if subpaths.is_empty() || rect.width == 0 || rect.height == 0 {
            return;
        }
        let outer_left = i64::from(rect.x) - i64::from(scroll_offset.x);
        let outer_top = i64::from(rect.y) - i64::from(scroll_offset.y);
        let outer_right = i64::from(rect.right()) - i64::from(scroll_offset.x);
        let outer_bottom = i64::from(rect.bottom()) - i64::from(scroll_offset.y);
        let Some((left, top, right, bottom)) =
            self.clipped_signed_bounds(outer_left, outer_top, outer_right, outer_bottom, clip)
        else {
            return;
        };
        for y in top..bottom {
            for x in left..right {
                let document_x = i64::from(x).saturating_add(i64::from(scroll_offset.x));
                let document_y = i64::from(y).saturating_add(i64::from(scroll_offset.y));
                if path_contains(subpaths, document_x as f64 + 0.5, document_y as f64 + 0.5) {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn svg_path_stroke(
        &mut self,
        rect: NativeRect,
        subpaths: &[NativeSvgSubpath],
        width: u32,
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        if subpaths.is_empty() || width == 0 || rect.width == 0 || rect.height == 0 {
            return;
        }
        let padding = i64::from(width.saturating_add(1) / 2);
        let outer_left = i64::from(rect.x)
            .saturating_sub(padding)
            .saturating_sub(i64::from(scroll_offset.x));
        let outer_top = i64::from(rect.y)
            .saturating_sub(padding)
            .saturating_sub(i64::from(scroll_offset.y));
        let outer_right = i64::from(rect.right())
            .saturating_add(padding)
            .saturating_sub(i64::from(scroll_offset.x));
        let outer_bottom = i64::from(rect.bottom())
            .saturating_add(padding)
            .saturating_sub(i64::from(scroll_offset.y));
        let Some((left, top, right, bottom)) =
            self.clipped_signed_bounds(outer_left, outer_top, outer_right, outer_bottom, clip)
        else {
            return;
        };
        let radius_squared = f64::from(width) * f64::from(width) / 4.0;
        for y in top..bottom {
            for x in left..right {
                let document_x = i64::from(x).saturating_add(i64::from(scroll_offset.x));
                let document_y = i64::from(y).saturating_add(i64::from(scroll_offset.y));
                let pixel_x = document_x as f64 + 0.5;
                let pixel_y = document_y as f64 + 0.5;
                if subpaths.iter().any(|subpath| {
                    subpath_stroke_contains(
                        &subpath.points,
                        subpath.closed,
                        pixel_x,
                        pixel_y,
                        radius_squared,
                    )
                }) {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn border_rect(
        &mut self,
        rect: NativeRect,
        radius: NativeBorderRadius,
        borders: super::paint::NativeBorderPaint,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        let outer_left = i64::from(rect.x) - i64::from(scroll_offset.x);
        let outer_top = i64::from(rect.y) - i64::from(scroll_offset.y);
        let outer_right = i64::from(rect.right()) - i64::from(scroll_offset.x);
        let outer_bottom = i64::from(rect.bottom()) - i64::from(scroll_offset.y);
        let Some((left, top, right, bottom)) =
            self.clipped_signed_bounds(outer_left, outer_top, outer_right, outer_bottom, clip)
        else {
            return;
        };
        let inner_rect = NativeRect {
            x: rect.x.saturating_add(borders.left.width),
            y: rect.y.saturating_add(borders.top.width),
            width: rect
                .width
                .saturating_sub(borders.left.width.saturating_add(borders.right.width)),
            height: rect
                .height
                .saturating_sub(borders.top.width.saturating_add(borders.bottom.width)),
        };
        let inner_radius = NativeBorderRadius {
            top_left: radius
                .top_left
                .saturating_sub(borders.top.width.max(borders.left.width)),
            top_right: radius
                .top_right
                .saturating_sub(borders.top.width.max(borders.right.width)),
            bottom_right: radius
                .bottom_right
                .saturating_sub(borders.bottom.width.max(borders.right.width)),
            bottom_left: radius
                .bottom_left
                .saturating_sub(borders.bottom.width.max(borders.left.width)),
        };
        for y in top..bottom {
            for x in left..right {
                let signed_x = i64::from(x);
                let signed_y = i64::from(y);
                let document_x = signed_x.saturating_add(i64::from(scroll_offset.x));
                let document_y = signed_y.saturating_add(i64::from(scroll_offset.y));
                let Some(document_x) = u32::try_from(document_x).ok() else {
                    continue;
                };
                let Some(document_y) = u32::try_from(document_y).ok() else {
                    continue;
                };
                let document_point = NativePoint {
                    x: document_x,
                    y: document_y,
                };
                if !rounded_rect_contains(rect, radius, document_point)
                    || rounded_rect_contains(inner_rect, inner_radius, document_point)
                {
                    continue;
                }
                let paint = if signed_y < outer_top.saturating_add(i64::from(borders.top.width)) {
                    Some((
                        borders.top,
                        i64::from(document_x.saturating_sub(rect.x)),
                        signed_y.saturating_sub(outer_top),
                        NativeBorderEdge::Top,
                    ))
                } else if signed_x >= outer_right.saturating_sub(i64::from(borders.right.width)) {
                    Some((
                        borders.right,
                        i64::from(document_y.saturating_sub(rect.y)),
                        outer_right.saturating_sub(1).saturating_sub(signed_x),
                        NativeBorderEdge::Right,
                    ))
                } else if signed_y >= outer_bottom.saturating_sub(i64::from(borders.bottom.width)) {
                    Some((
                        borders.bottom,
                        i64::from(document_x.saturating_sub(rect.x)),
                        outer_bottom.saturating_sub(1).saturating_sub(signed_y),
                        NativeBorderEdge::Bottom,
                    ))
                } else if signed_x < outer_left.saturating_add(i64::from(borders.left.width)) {
                    Some((
                        borders.left,
                        i64::from(document_y.saturating_sub(rect.y)),
                        signed_x.saturating_sub(outer_left),
                        NativeBorderEdge::Left,
                    ))
                } else {
                    None
                };
                if let Some((side, position, cross_position, edge)) = paint
                    && let Some(color) = Self::border_pixel(
                        side.style,
                        side.width,
                        position,
                        cross_position,
                        edge,
                        side.color,
                    )
                {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn border_pattern_paints(
        style: super::css::NativeBorderStyle,
        width: u32,
        position: i64,
    ) -> bool {
        if width == 0 || position < 0 {
            return false;
        }
        let position = u32::try_from(position).unwrap_or(u32::MAX);
        match style {
            super::css::NativeBorderStyle::Solid
            | super::css::NativeBorderStyle::Double
            | super::css::NativeBorderStyle::Groove
            | super::css::NativeBorderStyle::Ridge
            | super::css::NativeBorderStyle::Inset
            | super::css::NativeBorderStyle::Outset => true,
            super::css::NativeBorderStyle::Dashed => {
                let dash = width.saturating_mul(3).max(1);
                let gap = width.saturating_mul(2).max(1);
                let period = dash.saturating_add(gap);
                position % period < dash
            }
            super::css::NativeBorderStyle::Dotted => {
                let dot = width.max(1);
                let period = dot.saturating_mul(2);
                position % period < dot
            }
        }
    }

    fn border_pixel(
        style: super::css::NativeBorderStyle,
        width: u32,
        position: i64,
        cross_position: i64,
        edge: NativeBorderEdge,
        color: super::css::NativeColor,
    ) -> Option<super::css::NativeColor> {
        if width == 0 || position < 0 || cross_position < 0 {
            return None;
        }
        let cross_position = u32::try_from(cross_position).ok()?;
        if cross_position >= width {
            return None;
        }
        match style {
            super::css::NativeBorderStyle::Solid => Some(color),
            super::css::NativeBorderStyle::Dashed | super::css::NativeBorderStyle::Dotted => {
                Self::border_pattern_paints(style, width, position).then_some(color)
            }
            super::css::NativeBorderStyle::Double => {
                let paints = if width < 3 {
                    true
                } else {
                    let stripe = (width / 3).max(1);
                    cross_position < stripe || cross_position >= width.saturating_sub(stripe)
                };
                paints.then_some(color)
            }
            super::css::NativeBorderStyle::Groove | super::css::NativeBorderStyle::Ridge => {
                let outer_half = (width.saturating_add(1)) / 2;
                let outer = cross_position < outer_half;
                let light = match style {
                    super::css::NativeBorderStyle::Groove => !outer,
                    super::css::NativeBorderStyle::Ridge => outer,
                    _ => unreachable!(),
                };
                Some(Self::shade_border_color(color, light))
            }
            super::css::NativeBorderStyle::Inset | super::css::NativeBorderStyle::Outset => {
                let top_or_left = matches!(edge, NativeBorderEdge::Top | NativeBorderEdge::Left);
                let light = match style {
                    super::css::NativeBorderStyle::Inset => !top_or_left,
                    super::css::NativeBorderStyle::Outset => top_or_left,
                    _ => unreachable!(),
                };
                Some(Self::shade_border_color(color, light))
            }
        }
    }

    fn shade_border_color(color: super::css::NativeColor, light: bool) -> super::css::NativeColor {
        let shade = |channel: u8| {
            if light {
                channel.saturating_add(u8::MAX.saturating_sub(channel) / 2)
            } else {
                channel / 2
            }
        };
        super::css::NativeColor {
            red: shade(color.red),
            green: shade(color.green),
            blue: shade(color.blue),
            alpha: color.alpha,
        }
    }

    fn text_decoration_pattern_paints(
        style: NativeTextDecorationStyle,
        thickness: u32,
        position: i64,
    ) -> bool {
        match style {
            NativeTextDecorationStyle::Solid | NativeTextDecorationStyle::Double => true,
            NativeTextDecorationStyle::Dashed => {
                Self::border_pattern_paints(NativeBorderStyle::Dashed, thickness, position)
            }
            NativeTextDecorationStyle::Dotted => {
                Self::border_pattern_paints(NativeBorderStyle::Dotted, thickness, position)
            }
            NativeTextDecorationStyle::Wavy => true,
        }
    }

    fn text_decoration_vertical_offset(style: NativeTextDecorationStyle, position: i64) -> i64 {
        match style {
            NativeTextDecorationStyle::Wavy => {
                let phase =
                    usize::try_from(position).unwrap_or_default() % WAVY_DECORATION_PHASE.len();
                WAVY_DECORATION_PHASE[phase]
            }
            NativeTextDecorationStyle::Solid
            | NativeTextDecorationStyle::Dashed
            | NativeTextDecorationStyle::Dotted
            | NativeTextDecorationStyle::Double => 0,
        }
    }

    fn clipped_bounds(
        &self,
        rect: NativeRect,
        clip: Option<NativeRect>,
    ) -> Option<(u32, u32, u32, u32)> {
        let mut left = rect.x.min(self.width);
        let mut top = rect.y.min(self.height);
        let mut right = rect.right().min(self.width);
        let mut bottom = rect.bottom().min(self.height);
        if let Some(clip) = clip {
            left = left.max(clip.x.min(self.width));
            top = top.max(clip.y.min(self.height));
            right = right.min(clip.right().min(self.width));
            bottom = bottom.min(clip.bottom().min(self.height));
        }
        (left < right && top < bottom).then_some((left, top, right, bottom))
    }

    fn clipped_signed_bounds(
        &self,
        left: i64,
        top: i64,
        right: i64,
        bottom: i64,
        clip: Option<NativeRect>,
    ) -> Option<(u32, u32, u32, u32)> {
        let surface_width = i64::from(self.width);
        let surface_height = i64::from(self.height);
        let mut left = left.max(0).min(surface_width);
        let mut top = top.max(0).min(surface_height);
        let mut right = right.max(0).min(surface_width);
        let mut bottom = bottom.max(0).min(surface_height);
        if let Some(clip) = clip {
            left = left.max(i64::from(clip.x).min(surface_width));
            top = top.max(i64::from(clip.y).min(surface_height));
            right = right.min(i64::from(clip.right()).min(surface_width));
            bottom = bottom.min(i64::from(clip.bottom()).min(surface_height));
        }
        (left < right && top < bottom).then_some((
            u32::try_from(left).unwrap_or(u32::MAX),
            u32::try_from(top).unwrap_or(u32::MAX),
            u32::try_from(right).unwrap_or(u32::MAX),
            u32::try_from(bottom).unwrap_or(u32::MAX),
        ))
    }

    fn draw_text(
        &mut self,
        origin: super::layout::NativePoint,
        text: &str,
        paint: TextPaint,
        line_boundary: NativeTextLineBoundary,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        let origin_x = i64::from(origin.x) - i64::from(scroll_offset.x);
        let origin_y = i64::from(origin.y) - i64::from(scroll_offset.y);
        let mut run_width = 0u32;
        let characters = text.chars().collect::<Vec<_>>();
        let skip_all = matches!(
            paint.decoration_skip_spaces,
            NativeTextDecorationSkipSpaces::All
        );
        let skip_start = matches!(
            paint.decoration_skip_spaces,
            NativeTextDecorationSkipSpaces::Start | NativeTextDecorationSkipSpaces::StartAndEnd
        ) && line_boundary.starts_line;
        let skip_end = matches!(
            paint.decoration_skip_spaces,
            NativeTextDecorationSkipSpaces::End | NativeTextDecorationSkipSpaces::StartAndEnd
        ) && line_boundary.ends_line;
        let leading_space_count = if skip_start {
            characters
                .iter()
                .take_while(|character| is_text_decoration_whitespace(**character))
                .count()
        } else {
            0
        };
        let trailing_space_start = if skip_end {
            characters.len().saturating_sub(
                characters
                    .iter()
                    .rev()
                    .take_while(|character| is_text_decoration_whitespace(**character))
                    .count(),
            )
        } else {
            characters.len()
        };
        let mut glyph_ink =
            if matches!(paint.decoration_skip_ink, NativeTextDecorationSkipInk::Auto)
                && (paint.overline || paint.underline)
            {
                Some(Vec::new())
            } else {
                None
            };
        let mut skip_space_ranges = Vec::new();
        for (character_index, character) in characters.into_iter().enumerate() {
            let offset = i64::from(run_width);
            let glyph_origin_x = origin_x.saturating_add(offset);
            let character_advance = GLYPH_ADVANCE
                .saturating_add(paint.letter_spacing)
                .saturating_add(if character == ' ' {
                    paint.word_spacing.saturating_add(paint.justify_spacing)
                } else {
                    0
                });
            let decoration_whitespace = is_text_decoration_whitespace(character);
            let edge_space = decoration_whitespace
                && ((skip_start && character_index < leading_space_count)
                    || (skip_end && character_index >= trailing_space_start));
            if decoration_whitespace && (skip_all || edge_space) {
                skip_space_ranges.push((
                    offset.saturating_sub(i64::from(paint.letter_spacing)),
                    offset.saturating_add(i64::from(character_advance)),
                ));
            }
            if let Some(rows) = glyph_rows(character) {
                for (row, bits) in rows.into_iter().enumerate() {
                    let y = origin_y.saturating_add(i64::try_from(row).unwrap_or(i64::MAX));
                    let last_column = if paint.bold {
                        GLYPH_WIDTH
                    } else {
                        GLYPH_WIDTH.saturating_sub(1)
                    };
                    let italic_shift = if paint.italic {
                        ITALIC_ROW_SHIFTS.get(row).copied().unwrap_or_default()
                    } else {
                        0
                    };
                    for column in 0..=last_column {
                        let source_pixel =
                            column < GLYPH_WIDTH && bits & (1 << (GLYPH_WIDTH - 1 - column)) != 0;
                        let bold_neighbor =
                            paint.bold && column > 0 && bits & (1 << (GLYPH_WIDTH - column)) != 0;
                        if !source_pixel && !bold_neighbor {
                            continue;
                        }
                        if let Some(glyph_ink) = glyph_ink.as_mut() {
                            let relative_x = offset
                                .saturating_add(i64::from(column))
                                .saturating_add(i64::from(italic_shift));
                            if let Ok(relative_x) = usize::try_from(relative_x)
                                && let Some(required_len) = relative_x.checked_add(1)
                            {
                                if glyph_ink.len() < required_len {
                                    glyph_ink.resize(required_len, 0);
                                }
                                glyph_ink[relative_x] |= 1u8 << row;
                            }
                        }
                        if y < 0 || y >= i64::from(self.height) {
                            continue;
                        }
                        let x = glyph_origin_x
                            .saturating_add(i64::from(column))
                            .saturating_add(i64::from(italic_shift));
                        if x >= 0
                            && x < i64::from(self.width)
                            && clip.is_none_or(|clip| {
                                clip.contains(NativePoint {
                                    x: u32::try_from(x).unwrap_or(u32::MAX),
                                    y: u32::try_from(y).unwrap_or(u32::MAX),
                                })
                            })
                        {
                            self.blend_pixel(
                                u32::try_from(x).unwrap_or(u32::MAX),
                                u32::try_from(y).unwrap_or(u32::MAX),
                                paint.color,
                            );
                        }
                    }
                }
            }
            run_width = run_width.saturating_add(character_advance);
        }
        for (enabled, line_y, skip_ink) in [
            (paint.overline, origin_y.saturating_sub(1), true),
            (paint.line_through, origin_y.saturating_add(3), false),
            (
                paint.underline,
                origin_y
                    .saturating_add(i64::from(GLYPH_HEIGHT))
                    .saturating_add(i64::from(paint.underline_offset)),
                true,
            ),
        ] {
            if enabled {
                let band_count =
                    if matches!(paint.decoration_style, NativeTextDecorationStyle::Double) {
                        2
                    } else {
                        1
                    };
                for band_index in 0..band_count {
                    let band_offset = if band_index == 0 {
                        0
                    } else {
                        i64::from(paint.decoration_thickness).saturating_add(1)
                    };
                    for offset in 0..run_width {
                        let band_origin = line_y.saturating_add(band_offset).saturating_add(
                            Self::text_decoration_vertical_offset(
                                paint.decoration_style,
                                i64::from(offset),
                            ),
                        );
                        for thickness_offset in 0..paint.decoration_thickness {
                            let y = band_origin.saturating_add(i64::from(thickness_offset));
                            if y < 0 || y >= i64::from(self.height) {
                                continue;
                            }
                            let skips_space = skip_space_ranges.iter().any(|(start, end)| {
                                let offset = i64::from(offset);
                                offset >= *start && offset < *end
                            });
                            if skips_space {
                                continue;
                            }
                            let intersects_glyph = skip_ink
                                && matches!(
                                    paint.decoration_skip_ink,
                                    NativeTextDecorationSkipInk::Auto
                                )
                                && glyph_ink.as_ref().is_some_and(|glyph_ink| {
                                    let Ok(row) = usize::try_from(y.saturating_sub(origin_y))
                                    else {
                                        return false;
                                    };
                                    if row >= GLYPH_HEIGHT as usize {
                                        return false;
                                    }
                                    let Ok(offset) = usize::try_from(offset) else {
                                        return false;
                                    };
                                    glyph_ink
                                        .get(offset)
                                        .is_some_and(|bits| bits & (1u8 << row) != 0)
                                });
                            if intersects_glyph {
                                continue;
                            }
                            let x = origin_x.saturating_add(i64::from(offset));
                            if x >= 0
                                && x < i64::from(self.width)
                                && Self::text_decoration_pattern_paints(
                                    paint.decoration_style,
                                    paint.decoration_thickness,
                                    i64::from(offset),
                                )
                                && clip.is_none_or(|clip| {
                                    clip.contains(NativePoint {
                                        x: u32::try_from(x).unwrap_or(u32::MAX),
                                        y: u32::try_from(y).unwrap_or(u32::MAX),
                                    })
                                })
                            {
                                self.blend_pixel(
                                    u32::try_from(x).unwrap_or(u32::MAX),
                                    u32::try_from(y).unwrap_or(u32::MAX),
                                    paint.decoration_color,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn composite_layer(&mut self, layer: &Self, opacity: u8) {
        let width = usize::try_from(self.width).unwrap_or(1);
        for (pixel_index, pixel) in layer.rgba.chunks_exact(4).enumerate() {
            let color = super::css::NativeColor {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: multiply_alpha(pixel[3], opacity),
            };
            let x = u32::try_from(pixel_index % width).unwrap_or(u32::MAX);
            let y = u32::try_from(pixel_index / width).unwrap_or(u32::MAX);
            self.blend_pixel(x, y, color);
        }
    }

    fn blend_pixel(&mut self, x: u32, y: u32, color: super::css::NativeColor) {
        let Some(index) = self.pixel_index(x, y) else {
            return;
        };
        let source_alpha = u32::from(color.alpha);
        if source_alpha == 0 {
            return;
        }
        if source_alpha == u32::from(u8::MAX) {
            self.rgba[index..index + 4].copy_from_slice(&[
                color.red,
                color.green,
                color.blue,
                color.alpha,
            ]);
            return;
        }

        let destination_alpha = u32::from(self.rgba[index + 3]);
        let inverse_source_alpha = u32::from(u8::MAX) - source_alpha;
        let output_alpha_scaled =
            source_alpha * u32::from(u8::MAX) + destination_alpha * inverse_source_alpha;
        if output_alpha_scaled == 0 {
            self.rgba[index..index + 4].fill(0);
            return;
        }
        let channels = [color.red, color.green, color.blue];
        for (channel, source) in channels.into_iter().enumerate() {
            let destination = u32::from(self.rgba[index + channel]);
            let source_premultiplied = u32::from(source) * source_alpha * u32::from(u8::MAX);
            let destination_premultiplied = destination * destination_alpha * inverse_source_alpha;
            let output_premultiplied = source_premultiplied + destination_premultiplied;
            self.rgba[index + channel] = u8::try_from(
                (output_premultiplied + output_alpha_scaled / 2) / output_alpha_scaled,
            )
            .unwrap_or(u8::MAX);
        }
        self.rgba[index + 3] =
            u8::try_from((output_alpha_scaled + u32::from(u8::MAX) / 2) / u32::from(u8::MAX))
                .unwrap_or(u8::MAX);
    }

    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        let pixel = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.width).ok()?)?
            .checked_add(usize::try_from(x).ok()?)?;
        pixel.checked_mul(4)
    }
}

const fn multiply_alpha(source: u8, multiplier: u8) -> u8 {
    (((source as u16) * (multiplier as u16) + (u8::MAX as u16) / 2) / (u8::MAX as u16)) as u8
}

fn ellipse_contains(rect: NativeRect, x: f64, y: f64, inset: f64) -> bool {
    let radius_x = f64::from(rect.width) / 2.0 - inset;
    let radius_y = f64::from(rect.height) / 2.0 - inset;
    if radius_x <= 0.0 || radius_y <= 0.0 {
        return false;
    }
    let center_x = f64::from(rect.x) + f64::from(rect.width) / 2.0;
    let center_y = f64::from(rect.y) + f64::from(rect.height) / 2.0;
    let normalized_x = (x - center_x) / radius_x;
    let normalized_y = (y - center_y) / radius_y;
    normalized_x.mul_add(normalized_x, normalized_y * normalized_y) <= 1.0
}

fn polygon_contains(points: &[NativePoint], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut previous = points[points.len().saturating_sub(1)];
    for &current in points {
        let current_x = f64::from(current.x);
        let current_y = f64::from(current.y);
        let previous_x = f64::from(previous.x);
        let previous_y = f64::from(previous.y);
        if (current_y > y) != (previous_y > y) {
            let intersection = (previous_x - current_x)
                .mul_add((y - current_y) / (previous_y - current_y), current_x);
            if x < intersection {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn path_contains(subpaths: &[NativeSvgSubpath], x: f64, y: f64) -> bool {
    let mut inside = false;
    for subpath in subpaths {
        if subpath.points.len() >= 3 && polygon_contains(&subpath.points, x, y) {
            inside = !inside;
        }
    }
    inside
}

fn subpath_stroke_contains(
    points: &[NativePoint],
    closed: bool,
    x: f64,
    y: f64,
    radius_squared: f64,
) -> bool {
    if points.len() < 2 {
        return false;
    }
    let mut paints = points
        .windows(2)
        .any(|segment| distance_to_segment_squared(x, y, segment[0], segment[1]) <= radius_squared);
    if closed && !paints {
        let first = points[0];
        let last = points[points.len().saturating_sub(1)];
        paints = distance_to_segment_squared(x, y, last, first) <= radius_squared;
    }
    paints
}

fn distance_to_segment_squared(x: f64, y: f64, start: NativePoint, end: NativePoint) -> f64 {
    let start_x = f64::from(start.x);
    let start_y = f64::from(start.y);
    let delta_x = f64::from(end.x) - start_x;
    let delta_y = f64::from(end.y) - start_y;
    let length_squared = delta_x.mul_add(delta_x, delta_y * delta_y);
    let projection = if length_squared == 0.0 {
        0.0
    } else {
        ((x - start_x).mul_add(delta_x, (y - start_y) * delta_y) / length_squared).clamp(0.0, 1.0)
    };
    let nearest_x = projection.mul_add(delta_x, start_x);
    let nearest_y = projection.mul_add(delta_y, start_y);
    let distance_x = x - nearest_x;
    let distance_y = y - nearest_y;
    distance_x.mul_add(distance_x, distance_y * distance_y)
}

impl NativeDisplayList {
    /// Replay this immutable display list into a bounded logical surface.
    pub fn rasterize(&self) -> Result<NativeSurface, NativeEngineError> {
        NativeSurface::from_display_list(self)
    }
}

fn is_text_decoration_whitespace(character: char) -> bool {
    character.is_whitespace()
}

fn glyph_rows(character: char) -> Option<[u8; 7]> {
    let character = character.to_ascii_uppercase();
    Some(match character {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ' ' => [0; 7],
        '.' => [0, 0, 0, 0, 0, 0, 0b00100],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        ':' => [0, 0b00100, 0, 0, 0b00100, 0, 0],
        ',' => [0, 0, 0, 0, 0, 0b00100, 0b01000],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::native_engine::{
        NativeBorderPaint, NativeBorderPaintSide, NativeBorderStyle, NativeColor,
        NativeDisplayCommand, NativeDisplayList, NativeDocument, NativePoint,
        NativeTextDecorationSkipInk, NativeTextDecorationStyle, Viewport,
    };
    use std::io::Cursor;

    fn display_list(
        commands: Vec<NativeDisplayCommand>,
        width: u32,
        height: u32,
    ) -> NativeDisplayList {
        NativeDisplayList {
            revision: 1,
            viewport: Viewport {
                width,
                height,
                device_scale_factor_milli: 1000,
            },
            scroll_offset: NativePoint { x: 0, y: 0 },
            commands,
            text_run_boundaries: Vec::new(),
        }
    }

    fn uniform_border(width: u32, color: NativeColor) -> NativeBorderPaint {
        let side = NativeBorderPaintSide {
            width,
            style: NativeBorderStyle::Solid,
            color,
        };
        NativeBorderPaint {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    fn styled_border(
        width: u32,
        style: NativeBorderStyle,
        color: NativeColor,
    ) -> NativeBorderPaintSide {
        NativeBorderPaintSide {
            width,
            style,
            color,
        }
    }

    #[test]
    fn surface_encodes_deterministic_rgba_png() {
        let surface = NativeSurface::from_display_list(&display_list(
            vec![NativeDisplayCommand::Clear {
                color: NativeColor {
                    red: 12,
                    green: 34,
                    blue: 56,
                    alpha: 255,
                },
            }],
            2,
            1,
        ))
        .unwrap();
        let first = surface.to_png().unwrap();
        assert_eq!(first, surface.to_png().unwrap());
        assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");

        let decoder = png::Decoder::new(Cursor::new(first));
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size()];
        let output = reader.next_frame(&mut decoded).unwrap();
        assert_eq!((output.width, output.height), (2, 1));
        assert_eq!(
            &decoded[..output.buffer_size()],
            &[12, 34, 56, 255, 12, 34, 56, 255]
        );
    }

    #[test]
    fn surface_replays_clear_fill_and_source_over_alpha() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::FillRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 1,
                        y: 1,
                        width: 2,
                        height: 1,
                    },
                    radius: NativeBorderRadius::default(),
                    color: NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 128,
                    },
                    clip: None,
                },
            ],
            4,
            3,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.width(), 4);
        assert_eq!(surface.height(), 3);
        assert_eq!(surface.rgba().len(), 4 * 3 * 4);
        assert_eq!(surface.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(1, 1), Some([127, 127, 255, 255]));
        assert_eq!(surface.pixel(3, 2), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(4, 0), None);
    }

    #[test]
    fn surface_composites_nested_opacity_groups_inside_out() {
        let node = NativeDocument::empty().root();
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::BeginOpacityGroup {
                    node_id: node,
                    opacity: 128,
                },
                NativeDisplayCommand::FillRect {
                    node_id: node,
                    rect: NativeRect {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 1,
                    },
                    radius: NativeBorderRadius::default(),
                    color: NativeColor::RED,
                    clip: None,
                },
                NativeDisplayCommand::BeginOpacityGroup {
                    node_id: node,
                    opacity: 128,
                },
                NativeDisplayCommand::FillRect {
                    node_id: node,
                    rect: NativeRect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    radius: NativeBorderRadius::default(),
                    color: NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    },
                    clip: None,
                },
                NativeDisplayCommand::EndOpacityGroup { node_id: node },
                NativeDisplayCommand::EndOpacityGroup { node_id: node },
            ],
            2,
            1,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 0), Some([255, 127, 127, 255]));
        assert_eq!(surface.pixel(0, 0), Some([191, 127, 191, 255]));
    }

    #[test]
    fn surface_rejects_unbalanced_and_over_budget_opacity_groups() {
        let node = NativeDocument::empty().root();
        let unclosed = display_list(
            vec![NativeDisplayCommand::BeginOpacityGroup {
                node_id: node,
                opacity: 128,
            }],
            2,
            2,
        );
        assert!(matches!(
            unclosed.rasterize(),
            Err(NativeEngineError::InvalidConfiguration { field, .. })
                if field == "native opacity group"
        ));

        let mut commands = vec![NativeDisplayCommand::Clear {
            color: NativeColor::WHITE,
        }];
        for _ in 0..4 {
            commands.push(NativeDisplayCommand::BeginOpacityGroup {
                node_id: node,
                opacity: 128,
            });
        }
        let over_budget = display_list(commands, 2_000, 2_000);
        assert!(matches!(
            over_budget.rasterize(),
            Err(NativeEngineError::LimitExceeded { resource, .. })
                if resource == "native opacity layer pixels"
        ));
    }

    #[test]
    fn surface_draws_known_glyphs_and_clips_to_viewport() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 3, y: 2 },
                    text: "A?".into(),
                    truncated: false,
                    color: NativeColor::RED,
                    decoration_color: NativeColor::RED,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
            ],
            8,
            8,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(3, 2), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(4, 2), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(7, 2), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(7, 7), Some([255, 0, 0, 255]));
    }

    #[test]
    fn surface_bold_glyph_dilation_preserves_fixed_cell_advance() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 0 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    decoration_color: NativeColor::BLACK,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 10 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    decoration_color: NativeColor::BLACK,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: true,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
            ],
            8,
            18,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 1), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(1, 11), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(5, 11), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(6, 11), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_italic_glyph_shear_preserves_fixed_cell_advance() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 0 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    decoration_color: NativeColor::BLACK,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 10 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    decoration_color: NativeColor::BLACK,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: true,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 20 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 128,
                    },
                    decoration_color: NativeColor {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 128,
                    },
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: true,
                    italic: true,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
            ],
            10,
            28,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(1, 10), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(3, 10), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(0, 16), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(3, 20), Some([127, 127, 127, 255]));
        assert_eq!(surface.pixel(6, 20), Some([127, 127, 127, 255]));
        assert_eq!(surface.pixel(7, 20), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_draws_alpha_underlines_across_fixed_cells_and_clips_them() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 1, y: 0 },
                    text: "A?".into(),
                    truncated: false,
                    color: NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 128,
                    },
                    decoration_color: NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 128,
                    },
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: true,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: Some(NativeRect {
                        x: 2,
                        y: 7,
                        width: 4,
                        height: 1,
                    }),
                },
            ],
            8,
            8,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 7), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(2, 7), Some([127, 191, 127, 255]));
        assert_eq!(surface.pixel(5, 7), Some([127, 191, 127, 255]));
        assert_eq!(surface.pixel(6, 7), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_draws_overline_and_line_through_with_scroll_translation() {
        let root = NativeDocument::empty().root();
        let mut list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::TextRun {
                    node_id: root,
                    origin: NativePoint { x: 1, y: 10 },
                    text: "A?".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    decoration_color: NativeColor::BLACK,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: true,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: Some(NativeRect {
                        x: 2,
                        y: 9,
                        width: 4,
                        height: 1,
                    }),
                },
                NativeDisplayCommand::TextRun {
                    node_id: root,
                    origin: NativePoint { x: 1, y: 20 },
                    text: "A?".into(),
                    truncated: false,
                    color: NativeColor::RED,
                    decoration_color: NativeColor::RED,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: true,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: root,
                    origin: NativePoint { x: 1, y: 30 },
                    text: "A?".into(),
                    truncated: false,
                    color: NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 128,
                    },
                    decoration_color: NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 128,
                    },
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: true,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
            ],
            16,
            36,
        );
        list.scroll_offset = NativePoint { x: 0, y: 5 };

        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 4), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(2, 4), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(5, 4), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(6, 4), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(12, 18), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(13, 18), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(12, 32), Some([127, 191, 127, 255]));
    }

    #[test]
    fn surface_enforces_command_clip_rectangles() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::FillRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 0,
                        y: 0,
                        width: 4,
                        height: 3,
                    },
                    radius: NativeBorderRadius::default(),
                    color: NativeColor::RED,
                    clip: Some(NativeRect {
                        x: 1,
                        y: 1,
                        width: 2,
                        height: 1,
                    }),
                },
            ],
            4,
            3,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(2, 1), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(3, 1), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_replays_inside_border_ring_and_clip() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::BorderRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 1,
                        y: 1,
                        width: 4,
                        height: 3,
                    },
                    radius: NativeBorderRadius::default(),
                    borders: uniform_border(1, NativeColor::RED),
                    clip: Some(NativeRect {
                        x: 1,
                        y: 1,
                        width: 3,
                        height: 2,
                    }),
                },
            ],
            6,
            5,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(3, 1), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(1, 2), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(2, 2), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(4, 1), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(2, 3), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_replays_side_specific_borders_with_deterministic_corners() {
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::BorderRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 1,
                        y: 1,
                        width: 6,
                        height: 5,
                    },
                    radius: NativeBorderRadius::default(),
                    borders: NativeBorderPaint {
                        top: NativeBorderPaintSide {
                            width: 1,
                            style: NativeBorderStyle::Solid,
                            color: NativeColor::RED,
                        },
                        right: NativeBorderPaintSide {
                            width: 1,
                            style: NativeBorderStyle::Solid,
                            color: NativeColor {
                                red: 0,
                                green: 128,
                                blue: 0,
                                alpha: 255,
                            },
                        },
                        bottom: NativeBorderPaintSide {
                            width: 1,
                            style: NativeBorderStyle::Solid,
                            color: NativeColor {
                                red: 0,
                                green: 0,
                                blue: 255,
                                alpha: 255,
                            },
                        },
                        left: NativeBorderPaintSide {
                            width: 1,
                            style: NativeBorderStyle::Solid,
                            color: NativeColor::BLACK,
                        },
                    },
                    clip: None,
                },
            ],
            8,
            8,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(3, 1), Some([255, 0, 0, 255]));
        assert_eq!(surface.pixel(6, 3), Some([0, 128, 0, 255]));
        assert_eq!(surface.pixel(3, 5), Some([0, 0, 255, 255]));
        assert_eq!(surface.pixel(1, 3), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(3, 3), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_replays_dashed_and_dotted_borders_with_document_phase() {
        let mut list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::BorderRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 1,
                        y: 0,
                        width: 8,
                        height: 6,
                    },
                    radius: NativeBorderRadius::default(),
                    borders: NativeBorderPaint {
                        top: styled_border(1, NativeBorderStyle::Dashed, NativeColor::RED),
                        right: styled_border(
                            1,
                            NativeBorderStyle::Dotted,
                            NativeColor {
                                red: 0,
                                green: 128,
                                blue: 0,
                                alpha: 255,
                            },
                        ),
                        bottom: styled_border(
                            1,
                            NativeBorderStyle::Solid,
                            NativeColor {
                                red: 0,
                                green: 0,
                                blue: 255,
                                alpha: 255,
                            },
                        ),
                        left: styled_border(0, NativeBorderStyle::Solid, NativeColor::BLACK),
                    },
                    clip: None,
                },
            ],
            10,
            6,
        );
        let unscrolled = list.rasterize().unwrap();

        assert_eq!(unscrolled.pixel(1, 0), Some([255, 0, 0, 255]));
        assert_eq!(unscrolled.pixel(3, 0), Some([255, 0, 0, 255]));
        assert_eq!(unscrolled.pixel(4, 0), Some([255, 255, 255, 255]));
        assert_eq!(unscrolled.pixel(5, 0), Some([255, 255, 255, 255]));
        assert_eq!(unscrolled.pixel(6, 0), Some([255, 0, 0, 255]));
        assert_eq!(unscrolled.pixel(8, 0), Some([255, 0, 0, 255]));
        assert_eq!(unscrolled.pixel(8, 1), Some([255, 255, 255, 255]));
        assert_eq!(unscrolled.pixel(8, 2), Some([0, 128, 0, 255]));

        list.scroll_offset = NativePoint { x: 0, y: 1 };
        let scrolled = list.rasterize().unwrap();
        assert_eq!(scrolled.pixel(8, 0), unscrolled.pixel(8, 1));
        assert_eq!(scrolled.pixel(8, 1), unscrolled.pixel(8, 2));
    }

    #[test]
    fn surface_replays_rounded_fill_and_border_with_document_geometry() {
        let radius = NativeBorderRadius {
            top_left: 3,
            top_right: 3,
            bottom_right: 3,
            bottom_left: 3,
        };
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::FillRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 1,
                        y: 1,
                        width: 8,
                        height: 8,
                    },
                    radius,
                    color: NativeColor::RED,
                    clip: None,
                },
                NativeDisplayCommand::BorderRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 1,
                        y: 1,
                        width: 8,
                        height: 8,
                    },
                    radius,
                    borders: uniform_border(1, NativeColor::BLACK),
                    clip: None,
                },
            ],
            10,
            10,
        );
        let unscrolled = list.rasterize().unwrap();

        assert_eq!(unscrolled.pixel(1, 1), Some([255, 255, 255, 255]));
        assert_eq!(unscrolled.pixel(3, 1), Some([0, 0, 0, 255]));
        assert_eq!(unscrolled.pixel(1, 4), Some([0, 0, 0, 255]));
        assert_eq!(unscrolled.pixel(4, 4), Some([255, 0, 0, 255]));

        let mut scrolled_list = list;
        scrolled_list.scroll_offset = NativePoint { x: 0, y: 1 };
        let scrolled = scrolled_list.rasterize().unwrap();
        assert_eq!(scrolled.pixel(3, 0), unscrolled.pixel(3, 1));
        assert_eq!(scrolled.pixel(4, 3), unscrolled.pixel(4, 4));
    }

    #[test]
    fn rounded_mask_normalizes_radii_to_the_concrete_box() {
        let rect = NativeRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let radius = NativeBorderRadius {
            top_left: 100,
            top_right: 100,
            bottom_right: 100,
            bottom_left: 100,
        };
        assert!(!rounded_rect_contains(
            rect,
            radius,
            NativePoint { x: 0, y: 0 }
        ));
        assert!(rounded_rect_contains(
            rect,
            radius,
            NativePoint { x: 5, y: 5 }
        ));
        assert!(rounded_rect_contains(
            rect,
            radius,
            NativePoint { x: 0, y: 5 }
        ));
    }

    #[test]
    fn surface_translates_scrolled_geometry_without_pinning_offscreen_edges() {
        let mut list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                NativeDisplayCommand::BorderRect {
                    node_id: NativeDocument::empty().root(),
                    rect: NativeRect {
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 20,
                    },
                    radius: NativeBorderRadius::default(),
                    borders: uniform_border(2, NativeColor::RED),
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 0 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    decoration_color: NativeColor::BLACK,
                    decoration_style: NativeTextDecorationStyle::Solid,
                    decoration_skip_ink: NativeTextDecorationSkipInk::None,
                    decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                    decoration_thickness: 1,
                    underline_offset: 0,
                    underline: false,
                    overline: false,
                    line_through: false,
                    bold: false,
                    italic: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    justify_spacing: 0,
                    clip: None,
                },
            ],
            16,
            16,
        );
        list.scroll_offset = NativePoint { x: 0, y: 10 };

        let surface = NativeSurface::from_display_list(&list).unwrap();
        assert_eq!(surface.pixel(8, 0), Some([255, 255, 255, 255]));
        assert_eq!(surface.pixel(8, 8), Some([255, 0, 0, 255]));
    }

    #[test]
    fn surface_clamps_untrusted_decoration_thickness() {
        let list = display_list(
            vec![NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 0 },
                text: "A".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: u32::MAX,
                underline_offset: 0,
                underline: true,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            }],
            8,
            12,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(0, 7), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(0, 10), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(0, 11), Some([0, 0, 0, 0]));
    }

    #[test]
    fn surface_clamps_untrusted_underline_offset() {
        let list = display_list(
            vec![NativeDisplayCommand::TextRun {
                node_id: NativeDocument::empty().root(),
                origin: NativePoint { x: 0, y: 0 },
                text: " ".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Solid,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: i32::MAX,
                underline: true,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            }],
            8,
            12,
        );
        let surface = list.rasterize().unwrap();

        assert_eq!(surface.pixel(0, 7), Some([0, 0, 0, 0]));
        assert_eq!(surface.pixel(0, 10), Some([0, 0, 0, 0]));
        assert_eq!(surface.pixel(0, 11), Some([0, 0, 0, 255]));
    }

    #[test]
    fn surface_replays_double_decoration_as_two_thick_bands() {
        let node_id = NativeDocument::empty().root();
        let text_run =
            |origin: NativePoint, underline_offset: i32, line| NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text: " ".into(),
                truncated: false,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: NativeTextDecorationStyle::Double,
                decoration_skip_ink: NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 2,
                underline_offset,
                underline: line == "underline",
                overline: line == "overline",
                line_through: line == "line-through",
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            };
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                text_run(NativePoint { x: 0, y: 10 }, 0, "overline"),
                text_run(NativePoint { x: 0, y: 30 }, 0, "line-through"),
                text_run(NativePoint { x: 0, y: 50 }, -2, "underline"),
            ],
            8,
            64,
        );
        let surface = list.rasterize().unwrap();

        for y in [9, 10, 12, 13, 33, 34, 36, 37, 55, 56, 58, 59] {
            assert_eq!(surface.pixel(0, y), Some([0, 0, 0, 255]), "painted row {y}");
        }
        for y in [11, 35, 57] {
            assert_eq!(
                surface.pixel(0, y),
                Some([255, 255, 255, 255]),
                "gap row {y}"
            );
        }
    }

    #[test]
    fn surface_replays_wavy_decoration_with_fixed_phase_and_run_reset() {
        let node_id = NativeDocument::empty().root();
        let text_run = |origin: NativePoint, text: &str, thickness| NativeDisplayCommand::TextRun {
            node_id,
            origin,
            text: text.into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::BLACK,
            decoration_style: NativeTextDecorationStyle::Wavy,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            decoration_thickness: thickness,
            underline_offset: 0,
            underline: true,
            overline: false,
            line_through: false,
            bold: false,
            italic: false,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
            clip: None,
        };
        let list = display_list(
            vec![
                NativeDisplayCommand::Clear {
                    color: NativeColor::WHITE,
                },
                text_run(NativePoint { x: 0, y: 10 }, "  ", 2),
                text_run(NativePoint { x: 3, y: 30 }, " ", 1),
            ],
            16,
            48,
        );
        let surface = list.rasterize().unwrap();

        for (x, top) in [
            (0, 17),
            (1, 18),
            (2, 19),
            (3, 18),
            (4, 17),
            (5, 16),
            (6, 15),
            (7, 16),
            (8, 17),
            (9, 18),
            (10, 19),
            (11, 18),
        ] {
            assert_eq!(surface.pixel(x, top), Some([0, 0, 0, 255]), "top x={x}");
            assert_eq!(
                surface.pixel(x, top + 1),
                Some([0, 0, 0, 255]),
                "thickness x={x}"
            );
            assert_eq!(surface.pixel(x, top - 1), Some([255, 255, 255, 255]));
            assert_eq!(surface.pixel(x, top + 2), Some([255, 255, 255, 255]));
        }

        assert_eq!(surface.pixel(3, 37), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(3, 38), Some([255, 255, 255, 255]));
    }

    #[test]
    fn surface_skip_ink_auto_uses_the_existing_bold_italic_glyph_mask() {
        let node_id = NativeDocument::empty().root();
        let text_run = |skip_ink| NativeDisplayCommand::TextRun {
            node_id,
            origin: NativePoint { x: 0, y: 10 },
            text: "A".into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::RED,
            decoration_style: NativeTextDecorationStyle::Wavy,
            decoration_skip_ink: skip_ink,
            decoration_skip_spaces: NativeTextDecorationSkipSpaces::None,
            decoration_thickness: 1,
            underline_offset: -1,
            underline: true,
            overline: true,
            line_through: true,
            bold: true,
            italic: true,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
            clip: None,
        };
        let rasterize = |skip_ink| {
            display_list(
                vec![
                    NativeDisplayCommand::Clear {
                        color: NativeColor::WHITE,
                    },
                    text_run(skip_ink),
                ],
                8,
                20,
            )
            .rasterize()
            .unwrap()
        };
        let auto = rasterize(NativeTextDecorationSkipInk::Auto);
        let none = rasterize(NativeTextDecorationSkipInk::None);

        assert_eq!(auto.pixel(4, 16), Some([0, 0, 0, 255]));
        assert_eq!(none.pixel(4, 16), Some([255, 0, 0, 255]));
        assert_eq!(auto.pixel(5, 15), Some([0, 0, 0, 255]));
        assert_eq!(none.pixel(5, 15), Some([255, 0, 0, 255]));
        assert_eq!(auto.pixel(2, 11), Some([0, 0, 0, 255]));
        assert_eq!(none.pixel(2, 11), Some([255, 0, 0, 255]));
        assert_eq!(auto.pixel(1, 17), Some([255, 0, 0, 255]));
        assert_eq!(none.pixel(1, 17), Some([255, 0, 0, 255]));
        assert_eq!(auto.pixel(1, 14), Some([255, 0, 0, 255]));
        assert_eq!(none.pixel(1, 14), Some([255, 0, 0, 255]));
    }

    #[test]
    fn surface_skip_spaces_all_covers_space_and_authored_spacing_advances() {
        let node_id = NativeDocument::empty().root();
        let text_run = |skip_spaces| NativeDisplayCommand::TextRun {
            node_id,
            origin: NativePoint { x: 0, y: 10 },
            text: "A B".into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::RED,
            decoration_style: NativeTextDecorationStyle::Solid,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: skip_spaces,
            decoration_thickness: 1,
            underline_offset: 0,
            underline: true,
            overline: true,
            line_through: true,
            bold: false,
            italic: false,
            word_spacing: 2,
            letter_spacing: 1,
            justify_spacing: 2,
            clip: None,
        };
        let rasterize = |skip_spaces| {
            display_list(
                vec![
                    NativeDisplayCommand::Clear {
                        color: NativeColor::WHITE,
                    },
                    text_run(skip_spaces),
                ],
                32,
                24,
            )
            .rasterize()
            .unwrap()
        };
        let none = rasterize(NativeTextDecorationSkipSpaces::None);
        let all = rasterize(NativeTextDecorationSkipSpaces::All);

        for y in [9, 13, 17] {
            assert_eq!(none.pixel(6, y), Some([255, 0, 0, 255]), "none start y={y}");
            assert_eq!(none.pixel(17, y), Some([255, 0, 0, 255]), "none end y={y}");
            assert_eq!(
                all.pixel(6, y),
                Some([255, 255, 255, 255]),
                "all start y={y}"
            );
            assert_eq!(
                all.pixel(17, y),
                Some([255, 255, 255, 255]),
                "all end y={y}"
            );
            assert_eq!(all.pixel(0, y), Some([255, 0, 0, 255]), "all before y={y}");
            assert_eq!(all.pixel(18, y), Some([255, 0, 0, 255]), "all after y={y}");
        }
    }

    #[test]
    fn surface_skip_spaces_all_covers_unicode_whitespace_intervals() {
        let node_id = NativeDocument::empty().root();
        let text_run = |skip_spaces| NativeDisplayCommand::TextRun {
            node_id,
            origin: NativePoint { x: 0, y: 10 },
            text: "A\tB\u{00a0}C".into(),
            truncated: false,
            color: NativeColor::BLACK,
            decoration_color: NativeColor::RED,
            decoration_style: NativeTextDecorationStyle::Solid,
            decoration_skip_ink: NativeTextDecorationSkipInk::None,
            decoration_skip_spaces: skip_spaces,
            decoration_thickness: 1,
            underline_offset: 0,
            underline: true,
            overline: true,
            line_through: true,
            bold: false,
            italic: false,
            word_spacing: 0,
            letter_spacing: 0,
            justify_spacing: 0,
            clip: None,
        };
        let rasterize = |skip_spaces| {
            display_list(
                vec![
                    NativeDisplayCommand::Clear {
                        color: NativeColor::WHITE,
                    },
                    text_run(skip_spaces),
                ],
                48,
                24,
            )
            .rasterize()
            .unwrap()
        };
        let none = rasterize(NativeTextDecorationSkipSpaces::None);
        let all = rasterize(NativeTextDecorationSkipSpaces::All);

        for y in [9, 13, 17] {
            for x in [6, 18] {
                assert_eq!(
                    none.pixel(x, y),
                    Some([255, 0, 0, 255]),
                    "none whitespace x={x} y={y}"
                );
                assert_eq!(
                    all.pixel(x, y),
                    Some([255, 255, 255, 255]),
                    "all whitespace x={x} y={y}"
                );
            }
            for x in [0, 12, 24] {
                assert_eq!(
                    all.pixel(x, y),
                    Some([255, 0, 0, 255]),
                    "all non-whitespace x={x} y={y}"
                );
            }
        }
    }

    #[test]
    fn surface_rejects_unbounded_pixel_count_before_allocation() {
        let list = display_list(Vec::new(), 4096, 1025);
        assert!(matches!(
            list.rasterize(),
            Err(NativeEngineError::LimitExceeded { resource, .. })
                if resource == "native surface pixels"
        ));
    }
}
