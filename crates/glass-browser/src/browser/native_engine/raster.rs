use super::css::NativeBorderRadius;
use super::error::NativeEngineError;
use super::layout::{NativePoint, NativeRect, rounded_rect_contains};
use super::paint::{MAX_NATIVE_DISPLAY_COMMANDS, NativeDisplayCommand, NativeDisplayList};

/// Maximum number of logical pixels retained by one native software surface.
pub const MAX_NATIVE_SURFACE_PIXELS: usize = 4 * 1024 * 1024;
/// Maximum nesting depth for bounded opacity groups.
pub const MAX_NATIVE_OPACITY_GROUP_DEPTH: usize = 8;
/// Maximum logical pixels retained across the root surface and opacity layers.
pub const MAX_NATIVE_OPACITY_LAYER_PIXELS: usize = MAX_NATIVE_SURFACE_PIXELS * 4;
const GLYPH_WIDTH: u32 = 5;
const GLYPH_ADVANCE: u32 = 6;
const GLYPH_HEIGHT: u32 = 7;

#[derive(Debug, Clone, Copy)]
struct TextPaint {
    color: super::css::NativeColor,
    underline: bool,
    bold: bool,
    word_spacing: u32,
    letter_spacing: u32,
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
                    underline,
                    bold,
                    word_spacing,
                    letter_spacing,
                    clip,
                    ..
                } => {
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
                    Self::current_surface_mut(&mut surfaces)?.draw_text(
                        *origin,
                        text,
                        TextPaint {
                            color: *color,
                            underline: *underline,
                            bold: *bold,
                            word_spacing: *word_spacing,
                            letter_spacing: *letter_spacing,
                        },
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
                    Some((borders.top, i64::from(document_x.saturating_sub(rect.x))))
                } else if signed_x >= outer_right.saturating_sub(i64::from(borders.right.width)) {
                    Some((borders.right, i64::from(document_y.saturating_sub(rect.y))))
                } else if signed_y >= outer_bottom.saturating_sub(i64::from(borders.bottom.width)) {
                    Some((borders.bottom, i64::from(document_x.saturating_sub(rect.x))))
                } else if signed_x < outer_left.saturating_add(i64::from(borders.left.width)) {
                    Some((borders.left, i64::from(document_y.saturating_sub(rect.y))))
                } else {
                    None
                };
                if let Some((side, position)) = paint
                    && Self::border_pattern_paints(side.style, side.width, position)
                {
                    self.blend_pixel(x, y, side.color);
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
            super::css::NativeBorderStyle::Solid => true,
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
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        let origin_x = i64::from(origin.x) - i64::from(scroll_offset.x);
        let origin_y = i64::from(origin.y) - i64::from(scroll_offset.y);
        let mut run_width = 0u32;
        for character in text.chars() {
            let offset = i64::from(run_width);
            let glyph_origin_x = origin_x.saturating_add(offset);
            if let Some(rows) = glyph_rows(character) {
                for (row, bits) in rows.into_iter().enumerate() {
                    let y = origin_y.saturating_add(i64::try_from(row).unwrap_or(i64::MAX));
                    if y < 0 || y >= i64::from(self.height) {
                        continue;
                    }
                    let last_column = if paint.bold {
                        GLYPH_WIDTH
                    } else {
                        GLYPH_WIDTH.saturating_sub(1)
                    };
                    for column in 0..=last_column {
                        let source_pixel =
                            column < GLYPH_WIDTH && bits & (1 << (GLYPH_WIDTH - 1 - column)) != 0;
                        let bold_neighbor =
                            paint.bold && column > 0 && bits & (1 << (GLYPH_WIDTH - column)) != 0;
                        if !source_pixel && !bold_neighbor {
                            continue;
                        }
                        let x = glyph_origin_x.saturating_add(i64::from(column));
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
            run_width = run_width
                .saturating_add(GLYPH_ADVANCE)
                .saturating_add(paint.letter_spacing)
                .saturating_add(if character == ' ' {
                    paint.word_spacing
                } else {
                    0
                });
        }
        if paint.underline {
            let underline_y = origin_y.saturating_add(i64::from(GLYPH_HEIGHT));
            if underline_y >= 0 && underline_y < i64::from(self.height) {
                for offset in 0..run_width {
                    let x = origin_x.saturating_add(i64::from(offset));
                    if x >= 0
                        && x < i64::from(self.width)
                        && clip.is_none_or(|clip| {
                            clip.contains(NativePoint {
                                x: u32::try_from(x).unwrap_or(u32::MAX),
                                y: u32::try_from(underline_y).unwrap_or(u32::MAX),
                            })
                        })
                    {
                        self.blend_pixel(
                            u32::try_from(x).unwrap_or(u32::MAX),
                            u32::try_from(underline_y).unwrap_or(u32::MAX),
                            paint.color,
                        );
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

impl NativeDisplayList {
    /// Replay this immutable display list into a bounded logical surface.
    pub fn rasterize(&self) -> Result<NativeSurface, NativeEngineError> {
        NativeSurface::from_display_list(self)
    }
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
        NativeDisplayCommand, NativeDisplayList, NativeDocument, NativePoint, Viewport,
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
                    underline: false,
                    bold: false,
                    word_spacing: 0,
                    letter_spacing: 0,
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
                    underline: false,
                    bold: false,
                    word_spacing: 0,
                    letter_spacing: 0,
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 10 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
                    underline: false,
                    bold: true,
                    word_spacing: 0,
                    letter_spacing: 0,
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
                    underline: true,
                    bold: false,
                    word_spacing: 0,
                    letter_spacing: 0,
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
                    underline: false,
                    bold: false,
                    word_spacing: 0,
                    letter_spacing: 0,
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
    fn surface_rejects_unbounded_pixel_count_before_allocation() {
        let list = display_list(Vec::new(), 4096, 1025);
        assert!(matches!(
            list.rasterize(),
            Err(NativeEngineError::LimitExceeded { resource, .. })
                if resource == "native surface pixels"
        ));
    }
}
