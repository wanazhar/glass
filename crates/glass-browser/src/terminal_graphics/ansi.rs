//! Portable browser-frame rendering for true-color text terminals.
//!
//! Each terminal cell represents two sampled source pixels with an upper-half
//! block. Ratatui remains the screen owner, so its normal cell diffing works
//! through SSH, multiplexers, and Mosh without out-of-band image state.

use std::io::Cursor;

use super::GraphicsError;

const MAX_DECODED_PIXELS: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameFit {
    #[default]
    Contain,
    Cover,
    Actual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnsiCell {
    pub top: Rgb,
    pub bottom: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnsiUpdate {
    pub source_width: u32,
    pub source_height: u32,
    pub changed_cells: usize,
    pub total_cells: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AnsiCanvas {
    width: u16,
    height: u16,
    cells: Vec<AnsiCell>,
}

impl AnsiCanvas {
    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    pub fn cells(&self) -> &[AnsiCell] {
        &self.cells
    }

    pub fn clear(&mut self) {
        self.width = 0;
        self.height = 0;
        self.cells.clear();
    }

    pub fn update_png(
        &mut self,
        payload: &[u8],
        width: u16,
        height: u16,
        fit: FrameFit,
    ) -> Result<AnsiUpdate, GraphicsError> {
        if width == 0 || height == 0 {
            self.clear();
            return Ok(AnsiUpdate::default());
        }
        let image = DecodedImage::from_png(payload)?;
        let target_width = u32::from(width);
        let target_height = u32::from(height).saturating_mul(2);
        let mut next = Vec::with_capacity(usize::from(width) * usize::from(height));
        for row in 0..u32::from(height) {
            for column in 0..target_width {
                next.push(AnsiCell {
                    top: image.sample_to_target(column, row * 2, target_width, target_height, fit),
                    bottom: image.sample_to_target(
                        column,
                        row * 2 + 1,
                        target_width,
                        target_height,
                        fit,
                    ),
                });
            }
        }
        let changed_cells = if self.width == width && self.height == height {
            next.iter()
                .zip(&self.cells)
                .filter(|(next, previous)| next != previous)
                .count()
        } else {
            next.len()
        };
        self.width = width;
        self.height = height;
        self.cells = next;
        Ok(AnsiUpdate {
            source_width: image.width,
            source_height: image.height,
            changed_cells,
            total_cells: self.cells.len(),
        })
    }
}

struct DecodedImage {
    width: u32,
    height: u32,
    pixels: Vec<Rgb>,
}

impl DecodedImage {
    fn from_png(payload: &[u8]) -> Result<Self, GraphicsError> {
        let mut decoder = png::Decoder::new(Cursor::new(payload));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|error| GraphicsError::Invalid(format!("PNG header: {error}")))?;
        let info = reader.info();
        let pixels = u64::from(info.width).saturating_mul(u64::from(info.height));
        if pixels == 0 || pixels > MAX_DECODED_PIXELS {
            return Err(GraphicsError::Invalid(format!(
                "PNG dimensions {}x{} exceed the decoded pixel budget",
                info.width, info.height
            )));
        }
        let mut bytes = vec![0; reader.output_buffer_size()];
        let output = reader
            .next_frame(&mut bytes)
            .map_err(|error| GraphicsError::Invalid(format!("PNG frame: {error}")))?;
        let data = &bytes[..output.buffer_size()];
        let mut rgb = Vec::with_capacity(pixels as usize);
        match output.color_type {
            png::ColorType::Rgb => {
                rgb.extend(data.chunks_exact(3).map(|pixel| Rgb {
                    red: pixel[0],
                    green: pixel[1],
                    blue: pixel[2],
                }));
            }
            png::ColorType::Rgba => {
                rgb.extend(
                    data.chunks_exact(4)
                        .map(|pixel| composite(pixel[0], pixel[1], pixel[2], pixel[3])),
                );
            }
            png::ColorType::Grayscale => {
                rgb.extend(data.iter().copied().map(|value| Rgb {
                    red: value,
                    green: value,
                    blue: value,
                }));
            }
            png::ColorType::GrayscaleAlpha => {
                rgb.extend(
                    data.chunks_exact(2)
                        .map(|pixel| composite(pixel[0], pixel[0], pixel[0], pixel[1])),
                );
            }
            png::ColorType::Indexed => {
                return Err(GraphicsError::Invalid(
                    "PNG palette was not expanded by the decoder".into(),
                ));
            }
        }
        if rgb.len() != pixels as usize {
            return Err(GraphicsError::Invalid(
                "decoded PNG pixel count did not match its dimensions".into(),
            ));
        }
        Ok(Self {
            width: output.width,
            height: output.height,
            pixels: rgb,
        })
    }

    fn sample_to_target(
        &self,
        target_x: u32,
        target_y: u32,
        target_width: u32,
        target_height: u32,
        fit: FrameFit,
    ) -> Rgb {
        let source_width = f64::from(self.width);
        let source_height = f64::from(self.height);
        let target_width_f = f64::from(target_width);
        let target_height_f = f64::from(target_height);
        let (scale, blank_outside) = match fit {
            FrameFit::Contain => (
                (target_width_f / source_width).min(target_height_f / source_height),
                true,
            ),
            FrameFit::Cover => (
                (target_width_f / source_width).max(target_height_f / source_height),
                false,
            ),
            FrameFit::Actual => (1.0, false),
        };
        let displayed_width = source_width * scale;
        let displayed_height = source_height * scale;
        let offset_x = (target_width_f - displayed_width) / 2.0;
        let offset_y = (target_height_f - displayed_height) / 2.0;
        let source_x = (f64::from(target_x) + 0.5 - offset_x) / scale;
        let source_y = (f64::from(target_y) + 0.5 - offset_y) / scale;
        if blank_outside
            && (source_x < 0.0
                || source_y < 0.0
                || source_x >= source_width
                || source_y >= source_height)
        {
            return Rgb::default();
        }
        let x = source_x.floor().clamp(0.0, source_width - 1.0) as u32;
        let y = source_y.floor().clamp(0.0, source_height - 1.0) as u32;
        self.pixels[(y * self.width + x) as usize]
    }
}

fn composite(red: u8, green: u8, blue: u8, alpha: u8) -> Rgb {
    let alpha = u16::from(alpha);
    Rgb {
        red: ((u16::from(red) * alpha) / 255) as u8,
        green: ((u16::from(green) * alpha) / 255) as u8,
        blue: ((u16::from(blue) * alpha) / 255) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_2x2() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255])
                .unwrap();
        }
        bytes
    }

    #[test]
    fn half_block_canvas_preserves_top_and_bottom_pixels() {
        let mut canvas = AnsiCanvas::default();
        let update = canvas
            .update_png(&png_2x2(), 2, 1, FrameFit::Contain)
            .unwrap();
        assert_eq!(update.changed_cells, 2);
        assert_eq!(
            canvas.cells()[0].top,
            Rgb {
                red: 255,
                green: 0,
                blue: 0
            }
        );
        assert_eq!(
            canvas.cells()[0].bottom,
            Rgb {
                red: 0,
                green: 0,
                blue: 255
            }
        );
        assert_eq!(
            canvas.cells()[1].top,
            Rgb {
                red: 0,
                green: 255,
                blue: 0
            }
        );
    }

    #[test]
    fn identical_frame_reports_no_dirty_cells() {
        let mut canvas = AnsiCanvas::default();
        canvas
            .update_png(&png_2x2(), 2, 1, FrameFit::Cover)
            .unwrap();
        let update = canvas
            .update_png(&png_2x2(), 2, 1, FrameFit::Cover)
            .unwrap();
        assert_eq!(update.changed_cells, 0);
    }
}
