use super::error::NativeEngineError;
use super::layout::{NativePoint, NativeRect};
use super::paint::{MAX_NATIVE_DISPLAY_COMMANDS, NativeDisplayCommand, NativeDisplayList};

/// Maximum number of logical pixels retained by one native software surface.
pub const MAX_NATIVE_SURFACE_PIXELS: usize = 4 * 1024 * 1024;
const GLYPH_WIDTH: u32 = 5;
const GLYPH_ADVANCE: u32 = 6;

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
        let mut surface = Self {
            width,
            height,
            rgba: vec![0; byte_len],
        };
        let scroll_offset = display_list.scroll_offset;
        for command in &display_list.commands {
            match command {
                NativeDisplayCommand::Clear { color } => surface.clear(*color),
                NativeDisplayCommand::FillRect {
                    rect, color, clip, ..
                } => {
                    let Some(rect) = Self::translate_rect(*rect, scroll_offset) else {
                        continue;
                    };
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    surface.fill_rect(rect, *color, clip);
                }
                NativeDisplayCommand::BorderRect {
                    rect,
                    width,
                    color,
                    clip,
                    ..
                } => {
                    let Some(clip) = Self::translate_clip(*clip, scroll_offset) else {
                        continue;
                    };
                    surface.border_rect(*rect, *width, *color, clip, scroll_offset);
                }
                NativeDisplayCommand::TextRun {
                    origin,
                    text,
                    color,
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
                    surface.draw_text(*origin, text, *color, clip, scroll_offset);
                }
            }
        }
        Ok(surface)
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
        rect: NativeRect,
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
    ) {
        let Some((left, top, right, bottom)) = self.clipped_bounds(rect, clip) else {
            return;
        };
        for y in top..bottom {
            for x in left..right {
                self.blend_pixel(x, y, color);
            }
        }
    }

    fn border_rect(
        &mut self,
        rect: NativeRect,
        width: u32,
        color: super::css::NativeColor,
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
        let inner_left = outer_left.saturating_add(i64::from(width));
        let inner_top = outer_top.saturating_add(i64::from(width));
        let inner_right = outer_right.saturating_sub(i64::from(width));
        let inner_bottom = outer_bottom.saturating_sub(i64::from(width));
        for y in top..bottom {
            for x in left..right {
                let signed_x = i64::from(x);
                let signed_y = i64::from(y);
                if signed_x < inner_left
                    || signed_x >= inner_right
                    || signed_y < inner_top
                    || signed_y >= inner_bottom
                {
                    self.blend_pixel(x, y, color);
                }
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
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
        scroll_offset: NativePoint,
    ) {
        let origin_x = i64::from(origin.x) - i64::from(scroll_offset.x);
        let origin_y = i64::from(origin.y) - i64::from(scroll_offset.y);
        for (index, character) in text.chars().enumerate() {
            let offset = i64::try_from(index)
                .unwrap_or(i64::MAX)
                .saturating_mul(i64::from(GLYPH_ADVANCE));
            let glyph_origin_x = origin_x.saturating_add(offset);
            if let Some(rows) = glyph_rows(character) {
                for (row, bits) in rows.into_iter().enumerate() {
                    let y = origin_y.saturating_add(i64::try_from(row).unwrap_or(i64::MAX));
                    if y < 0 || y >= i64::from(self.height) {
                        continue;
                    }
                    for column in 0..GLYPH_WIDTH {
                        if bits & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
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
                                color,
                            );
                        }
                    }
                }
            }
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
        NativeColor, NativeDisplayCommand, NativeDisplayList, NativeDocument, NativePoint, Viewport,
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
                    width: 1,
                    color: NativeColor::RED,
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
                    width: 2,
                    color: NativeColor::RED,
                    clip: None,
                },
                NativeDisplayCommand::TextRun {
                    node_id: NativeDocument::empty().root(),
                    origin: NativePoint { x: 0, y: 0 },
                    text: "A".into(),
                    truncated: false,
                    color: NativeColor::BLACK,
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
