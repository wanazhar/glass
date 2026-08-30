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
        for command in &display_list.commands {
            match command {
                NativeDisplayCommand::Clear { color } => surface.clear(*color),
                NativeDisplayCommand::FillRect {
                    rect, color, clip, ..
                } => surface.fill_rect(*rect, *color, *clip),
                NativeDisplayCommand::BorderRect {
                    rect,
                    width,
                    color,
                    clip,
                    ..
                } => surface.border_rect(*rect, *width, *color, *clip),
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
                    surface.draw_text(*origin, text, *color, *clip);
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
    ) {
        let Some((left, top, right, bottom)) = self.clipped_bounds(rect, clip) else {
            return;
        };
        let inner_left = rect.x.saturating_add(width);
        let inner_top = rect.y.saturating_add(width);
        let inner_right = rect.right().saturating_sub(width);
        let inner_bottom = rect.bottom().saturating_sub(width);
        for y in top..bottom {
            for x in left..right {
                if x < inner_left || x >= inner_right || y < inner_top || y >= inner_bottom {
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

    fn draw_text(
        &mut self,
        origin: super::layout::NativePoint,
        text: &str,
        color: super::css::NativeColor,
        clip: Option<NativeRect>,
    ) {
        for (index, character) in text.chars().enumerate() {
            let offset = u32::try_from(index)
                .unwrap_or(u32::MAX)
                .saturating_mul(GLYPH_ADVANCE);
            let glyph_origin = super::layout::NativePoint {
                x: origin.x.saturating_add(offset),
                y: origin.y,
            };
            if let Some(rows) = glyph_rows(character) {
                for (row, bits) in rows.into_iter().enumerate() {
                    let y = glyph_origin
                        .y
                        .saturating_add(u32::try_from(row).unwrap_or(u32::MAX));
                    if y >= self.height {
                        continue;
                    }
                    for column in 0..GLYPH_WIDTH {
                        if bits & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
                            continue;
                        }
                        let x = glyph_origin.x.saturating_add(column);
                        if x < self.width
                            && clip.is_none_or(|clip| clip.contains(NativePoint { x, y }))
                        {
                            self.blend_pixel(x, y, color);
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
    fn surface_rejects_unbounded_pixel_count_before_allocation() {
        let list = display_list(Vec::new(), 4096, 1025);
        assert!(matches!(
            list.rasterize(),
            Err(NativeEngineError::LimitExceeded { resource, .. })
                if resource == "native surface pixels"
        ));
    }
}
