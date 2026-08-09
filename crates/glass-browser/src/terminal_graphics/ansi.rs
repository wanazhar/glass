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
    scratch: Vec<AnsiCell>,
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
        self.scratch.clear();
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
        let sampling = image.sampling_plan(target_width, target_height, fit);
        self.scratch.clear();
        self.scratch
            .reserve(usize::from(width) * usize::from(height));
        for row in 0..u32::from(height) {
            for column in 0..target_width {
                self.scratch.push(AnsiCell {
                    top: image
                        .sample_mapped(sampling.x[column as usize], sampling.y[(row * 2) as usize]),
                    bottom: image.sample_mapped(
                        sampling.x[column as usize],
                        sampling.y[(row * 2 + 1) as usize],
                    ),
                });
            }
        }
        let changed_cells = if self.width == width && self.height == height {
            self.scratch
                .iter()
                .zip(&self.cells)
                .filter(|(next, previous)| next != previous)
                .count()
        } else {
            self.scratch.len()
        };
        self.width = width;
        self.height = height;
        std::mem::swap(&mut self.cells, &mut self.scratch);
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
    color_type: png::ColorType,
    bytes: Vec<u8>,
}

struct SamplingPlan {
    x: Vec<Option<u32>>,
    y: Vec<Option<u32>>,
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
        if output.color_type == png::ColorType::Indexed {
            return Err(GraphicsError::Invalid(
                "PNG palette was not expanded by the decoder".into(),
            ));
        }
        bytes.truncate(output.buffer_size());
        let expected_bytes = (pixels as usize).saturating_mul(output.color_type.samples());
        if bytes.len() != expected_bytes {
            return Err(GraphicsError::Invalid(
                "decoded PNG byte count did not match its dimensions".into(),
            ));
        }
        Ok(Self {
            width: output.width,
            height: output.height,
            color_type: output.color_type,
            bytes,
        })
    }

    fn sampling_plan(&self, target_width: u32, target_height: u32, fit: FrameFit) -> SamplingPlan {
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
        SamplingPlan {
            x: sample_axis(target_width, source_width, offset_x, scale, blank_outside),
            y: sample_axis(target_height, source_height, offset_y, scale, blank_outside),
        }
    }

    fn sample_mapped(&self, x: Option<u32>, y: Option<u32>) -> Rgb {
        let (Some(x), Some(y)) = (x, y) else {
            return Rgb::default();
        };
        let samples = self.color_type.samples();
        let offset = ((y * self.width + x) as usize).saturating_mul(samples);
        let pixel = &self.bytes[offset..offset + samples];
        match self.color_type {
            png::ColorType::Rgb => Rgb {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
            },
            png::ColorType::Rgba => composite(pixel[0], pixel[1], pixel[2], pixel[3]),
            png::ColorType::Grayscale => Rgb {
                red: pixel[0],
                green: pixel[0],
                blue: pixel[0],
            },
            png::ColorType::GrayscaleAlpha => composite(pixel[0], pixel[0], pixel[0], pixel[1]),
            png::ColorType::Indexed => Rgb::default(),
        }
    }
}

fn sample_axis(
    target_extent: u32,
    source_extent: f64,
    offset: f64,
    scale: f64,
    blank_outside: bool,
) -> Vec<Option<u32>> {
    (0..target_extent)
        .map(|target| {
            let source = (f64::from(target) + 0.5 - offset) / scale;
            if blank_outside && (source < 0.0 || source >= source_extent) {
                None
            } else {
                Some(source.floor().clamp(0.0, source_extent - 1.0) as u32)
            }
        })
        .collect()
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

    fn png_1x1(color: png::ColorType, pixel: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixel).unwrap();
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

    #[test]
    fn direct_sampling_preserves_alpha_and_grayscale_conversion() {
        let mut canvas = AnsiCanvas::default();
        canvas
            .update_png(
                &png_1x1(png::ColorType::Rgba, &[200, 100, 50, 128]),
                1,
                1,
                FrameFit::Cover,
            )
            .unwrap();
        assert_eq!(
            canvas.cells()[0].top,
            Rgb {
                red: 100,
                green: 50,
                blue: 25,
            }
        );

        canvas
            .update_png(
                &png_1x1(png::ColorType::GrayscaleAlpha, &[80, 128]),
                1,
                1,
                FrameFit::Cover,
            )
            .unwrap();
        assert_eq!(
            canvas.cells()[0].top,
            Rgb {
                red: 40,
                green: 40,
                blue: 40,
            }
        );
    }
}
