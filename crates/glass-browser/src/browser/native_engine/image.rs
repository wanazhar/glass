use super::config::{NativeEngineLimits, Viewport};
use super::dom::NativeDocument;
use base64::Engine as _;
use gif::{ColorOutput, DecodeOptions, DisposalMethod, MemoryLimit, Repeat};
use image_webp::{LoopCount, WebPDecoder};
use png::{BlendOp, ColorType, DisposeOp};
use std::io::Cursor;
use std::num::NonZeroU64;
use std::sync::OnceLock;
use std::time::Instant;
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};

const DEFAULT_NATIVE_SVG_IMAGE_WIDTH: u32 = 300;
const DEFAULT_NATIVE_SVG_IMAGE_HEIGHT: u32 = 150;
const MAX_NATIVE_SVG_IMAGE_DECODE_DEPTH: usize = 1;

thread_local! {
    static NATIVE_SVG_IMAGE_DECODE_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Maximum decoded RGBA bytes retained for one native inline image.
pub(crate) const MAX_NATIVE_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum decoded pixels retained for one native inline image.
pub(crate) const MAX_NATIVE_IMAGE_PIXELS: usize = MAX_NATIVE_IMAGE_BYTES / 4;
/// Maximum decoded RGBA bytes transferred from the content process for one
/// external image resource.
pub(crate) const MAX_NATIVE_IMAGE_TRANSFER_BYTES: usize = 512 * 1024;
/// Maximum decoded pixels transferred from the content process for one
/// external image resource.
pub(crate) const MAX_NATIVE_IMAGE_TRANSFER_PIXELS: usize = MAX_NATIVE_IMAGE_TRANSFER_BYTES / 4;
/// Maximum number of decoded animation frames retained for one image.
pub(crate) const MAX_NATIVE_IMAGE_FRAMES: usize = 64;
/// Maximum delay accepted for one decoded animation frame.
pub(crate) const MAX_NATIVE_IMAGE_FRAME_DELAY_MS: u32 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeImageFrame {
    pub(crate) delay_ms: u32,
    pub(crate) pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
    pub(crate) frames: Vec<NativeImageFrame>,
    pub(crate) loop_count: Option<u32>,
}

impl NativeImage {
    pub(crate) fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels,
            frames: Vec::new(),
            loop_count: None,
        }
    }

    pub(crate) fn with_frames(
        width: u32,
        height: u32,
        frames: Vec<NativeImageFrame>,
        loop_count: Option<u32>,
    ) -> Option<Self> {
        if frames.is_empty()
            || frames.len() > MAX_NATIVE_IMAGE_FRAMES
            || frames.iter().any(|frame| {
                frame.delay_ms == 0
                    || frame.delay_ms > MAX_NATIVE_IMAGE_FRAME_DELAY_MS
                    || frame.pixels.len()
                        != usize::try_from(width)
                            .ok()
                            .and_then(|width| {
                                usize::try_from(height)
                                    .ok()
                                    .and_then(|height| width.checked_mul(height))
                            })
                            .and_then(|pixels| pixels.checked_mul(4))
                            .unwrap_or(usize::MAX)
            })
        {
            return None;
        }
        let pixels = frames.first()?.pixels.clone();
        Some(Self {
            width,
            height,
            pixels,
            frames,
            loop_count,
        })
    }

    pub(crate) fn current_pixels(&self) -> &[u8] {
        self.frame_pixels_at(animation_elapsed_ms())
    }

    pub(crate) fn animation_elapsed_ms(&self) -> Option<u64> {
        (!self.frames.is_empty()).then(animation_elapsed_ms)
    }

    pub(crate) fn decoded_bytes(&self) -> Option<usize> {
        if self.frames.is_empty() {
            return Some(self.pixels.len());
        }
        self.frames
            .iter()
            .try_fold(self.pixels.len(), |total, frame| {
                total.checked_add(frame.pixels.len())
            })
    }

    fn frame_pixels_at(&self, elapsed_ms: u64) -> &[u8] {
        if self.frames.is_empty() {
            return &self.pixels;
        }
        let cycle_ms = self.frames.iter().fold(0_u64, |total, frame| {
            total.saturating_add(u64::from(frame.delay_ms))
        });
        if cycle_ms == 0 {
            return &self.frames[0].pixels;
        }
        if let Some(repeats) = self.loop_count {
            let total_ms = cycle_ms.saturating_mul(u64::from(repeats).saturating_add(1));
            if elapsed_ms >= total_ms {
                return &self.frames.last().expect("validated image frames").pixels;
            }
        }
        let position = elapsed_ms % cycle_ms;
        let mut elapsed = 0_u64;
        self.frames
            .iter()
            .find(|frame| {
                elapsed = elapsed.saturating_add(u64::from(frame.delay_ms));
                position < elapsed
            })
            .unwrap_or_else(|| self.frames.last().expect("validated image frames"))
            .pixels
            .as_slice()
    }
}

fn animation_elapsed_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeImageResource {
    pub(crate) source: String,
    pub(crate) image: NativeImage,
}

pub(crate) fn decode_data_image(source: &str) -> Option<NativeImage> {
    let prefix = source.get(..5)?;
    if !prefix.eq_ignore_ascii_case("data:") {
        return None;
    }
    let (metadata, payload) = source.get(5..)?.split_once(',')?;
    let mut metadata_parts = metadata.split(';');
    let media_type = metadata_parts.next().unwrap_or_default();
    if !matches_ignore_ascii_case(
        media_type,
        &[
            "image/png",
            "image/apng",
            "image/jpeg",
            "image/webp",
            "image/gif",
            "image/svg+xml",
        ],
    ) {
        return None;
    }
    let is_base64 = metadata_parts.any(|part| part.eq_ignore_ascii_case("base64"));
    let bytes = if is_base64 {
        let max_encoded = (MAX_NATIVE_IMAGE_BYTES.saturating_add(2) / 3).saturating_mul(4);
        if payload.len() > max_encoded {
            return None;
        }
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .ok()?
    } else {
        percent_decode_bytes(payload)?
    };
    if bytes.len() > MAX_NATIVE_IMAGE_BYTES {
        return None;
    }
    decode_image_bytes(&bytes, media_type, MAX_NATIVE_IMAGE_BYTES)
}

pub(crate) fn image_dimensions_from_source(source: &str) -> Option<(u32, u32)> {
    decode_data_image(source).map(|image| (image.width, image.height))
}

pub(crate) fn decode_png_bytes(bytes: &[u8], max_decoded_bytes: usize) -> Option<NativeImage> {
    if bytes.is_empty() || max_decoded_bytes < 4 {
        return None;
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let width = reader.info().width;
    let height = reader.info().height;
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let max_pixels = (max_decoded_bytes / 4).min(MAX_NATIVE_IMAGE_PIXELS);
    if width == 0 || height == 0 || pixel_count > max_pixels {
        return None;
    }
    if reader.info().animation_control().is_some() {
        return decode_apng_frames(reader, width, height, max_decoded_bytes);
    }
    let output_size = reader.output_buffer_size();
    if output_size > max_decoded_bytes {
        return None;
    }
    let mut decoded = vec![0; output_size];
    let output = reader.next_frame(&mut decoded).ok()?;
    if output.width != width || output.height != height {
        return None;
    }
    let raw = decoded.get(..output.buffer_size())?;
    let pixels = png_pixels_to_rgba(raw, output.color_type)?;
    (pixels.len() == pixel_count.checked_mul(4)? && pixels.len() <= max_decoded_bytes)
        .then(|| NativeImage::new(width, height, pixels))
}

fn png_pixels_to_rgba(raw: &[u8], color_type: ColorType) -> Option<Vec<u8>> {
    Some(match color_type {
        ColorType::Rgba => raw.to_vec(),
        ColorType::Rgb => raw
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], u8::MAX])
            .collect(),
        ColorType::GrayscaleAlpha => raw
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        ColorType::Grayscale => raw
            .iter()
            .flat_map(|value| [*value, *value, *value, u8::MAX])
            .collect(),
        ColorType::Indexed => return None,
    })
}

fn decode_apng_frames(
    mut reader: png::Reader<Cursor<&[u8]>>,
    width: u32,
    height: u32,
    max_decoded_bytes: usize,
) -> Option<NativeImage> {
    let animation = reader.info().animation_control().copied()?;
    let frame_count = usize::try_from(animation.num_frames).ok()?;
    if frame_count == 0 || frame_count > MAX_NATIVE_IMAGE_FRAMES {
        return None;
    }
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let max_pixels = (max_decoded_bytes / 4).min(MAX_NATIVE_IMAGE_PIXELS);
    if width == 0 || height == 0 || pixel_count > max_pixels {
        return None;
    }
    let canvas_bytes = pixel_count.checked_mul(4)?;
    let retained_bytes = frame_count.checked_add(1)?.checked_mul(canvas_bytes)?;
    if retained_bytes > max_decoded_bytes {
        return None;
    }
    let output_size = reader.output_buffer_size();
    if output_size > max_decoded_bytes {
        return None;
    }
    let separate_default_image = reader.info().frame_control().is_none();
    let mut decoded = vec![0; output_size];
    if separate_default_image {
        let output = reader.next_frame(&mut decoded).ok()?;
        if output.width != width || output.height != height {
            return None;
        }
    }

    let canvas_width = usize::try_from(width).ok()?;
    let canvas_height = usize::try_from(height).ok()?;
    let mut canvas = vec![0; canvas_bytes];
    let mut frames = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let output = reader.next_frame(&mut decoded).ok()?;
        let frame_control = reader.info().frame_control().copied()?;
        let frame_width = usize::try_from(frame_control.width).ok()?;
        let frame_height = usize::try_from(frame_control.height).ok()?;
        let left = usize::try_from(frame_control.x_offset).ok()?;
        let top = usize::try_from(frame_control.y_offset).ok()?;
        if frame_width == 0
            || frame_height == 0
            || left.checked_add(frame_width)? > canvas_width
            || top.checked_add(frame_height)? > canvas_height
            || output.width != frame_control.width
            || output.height != frame_control.height
        {
            return None;
        }
        let raw = decoded.get(..output.buffer_size())?;
        let frame_pixels = png_pixels_to_rgba(raw, output.color_type)?;
        let frame_bytes = frame_width.checked_mul(frame_height)?.checked_mul(4)?;
        if frame_pixels.len() != frame_bytes {
            return None;
        }
        let previous = (frame_control.dispose_op == DisposeOp::Previous).then(|| canvas.clone());
        composite_apng_frame(
            &mut canvas,
            canvas_width,
            left,
            top,
            frame_width,
            frame_height,
            &frame_pixels,
            frame_control.blend_op,
        )?;
        frames.push(NativeImageFrame {
            delay_ms: normalized_apng_delay_ms(frame_control.delay_num, frame_control.delay_den),
            pixels: canvas.clone(),
        });
        match frame_control.dispose_op {
            DisposeOp::None => {}
            DisposeOp::Background => {
                clear_apng_rect(
                    &mut canvas,
                    canvas_width,
                    left,
                    top,
                    frame_width,
                    frame_height,
                )?;
            }
            DisposeOp::Previous => {
                canvas = previous?;
            }
        }
    }
    let loop_count = (animation.num_plays != 0).then_some(animation.num_plays);
    let image = NativeImage::with_frames(width, height, frames, loop_count)?;
    (image.decoded_bytes()? <= max_decoded_bytes).then_some(image)
}

fn composite_apng_frame(
    canvas: &mut [u8],
    canvas_width: usize,
    left: usize,
    top: usize,
    frame_width: usize,
    frame_height: usize,
    frame_pixels: &[u8],
    blend_op: BlendOp,
) -> Option<()> {
    let frame_row_bytes = frame_width.checked_mul(4)?;
    for row in 0..frame_height {
        let frame_start = row.checked_mul(frame_row_bytes)?;
        let canvas_start = top
            .checked_add(row)?
            .checked_mul(canvas_width)?
            .checked_add(left)?
            .checked_mul(4)?;
        let source_row =
            frame_pixels.get(frame_start..frame_start.checked_add(frame_row_bytes)?)?;
        let destination_row =
            canvas.get_mut(canvas_start..canvas_start.checked_add(frame_row_bytes)?)?;
        if blend_op == BlendOp::Source {
            destination_row.copy_from_slice(source_row);
        } else {
            for (destination, source) in destination_row
                .chunks_exact_mut(4)
                .zip(source_row.chunks_exact(4))
            {
                blend_apng_pixel(destination, source);
            }
        }
    }
    Some(())
}

fn blend_apng_pixel(destination: &mut [u8], source: &[u8]) {
    let source_alpha = u32::from(source[3]);
    if source_alpha == 0 {
        return;
    }
    if source_alpha == u32::from(u8::MAX) {
        destination.copy_from_slice(source);
        return;
    }
    let destination_alpha = u32::from(destination[3]);
    let output_alpha = source_alpha
        + (destination_alpha
            .saturating_mul(u32::from(u8::MAX) - source_alpha)
            .saturating_add(127)
            / u32::from(u8::MAX));
    if output_alpha == 0 {
        destination.fill(0);
        return;
    }
    let denominator = u64::from(output_alpha) * u64::from(u8::MAX);
    for channel in 0..3 {
        let source_weight = u64::from(source[channel]) * u64::from(source_alpha) * 255;
        let destination_weight = u64::from(destination[channel])
            * u64::from(destination_alpha)
            * u64::from(u32::from(u8::MAX) - source_alpha);
        destination[channel] =
            ((source_weight + destination_weight + denominator / 2) / denominator) as u8;
    }
    destination[3] = u8::try_from(output_alpha).unwrap_or(u8::MAX);
}

fn normalized_apng_delay_ms(numerator: u16, denominator: u16) -> u32 {
    let denominator = u64::from(if denominator == 0 { 100 } else { denominator });
    let milliseconds = u64::from(numerator)
        .saturating_mul(1000)
        .saturating_add(denominator / 2)
        / denominator;
    u32::try_from(milliseconds.max(1))
        .unwrap_or(MAX_NATIVE_IMAGE_FRAME_DELAY_MS)
        .min(MAX_NATIVE_IMAGE_FRAME_DELAY_MS)
}

fn clear_apng_rect(
    canvas: &mut [u8],
    canvas_width: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
) -> Option<()> {
    for row in top..top.checked_add(height)? {
        let start = row
            .checked_mul(canvas_width)?
            .checked_add(left)?
            .checked_mul(4)?;
        canvas
            .get_mut(start..start.checked_add(width.checked_mul(4)?)?)?
            .fill(0);
    }
    Some(())
}

pub(crate) fn decode_image_bytes(
    bytes: &[u8],
    media_type: &str,
    max_decoded_bytes: usize,
) -> Option<NativeImage> {
    let decode_depth = NATIVE_SVG_IMAGE_DECODE_DEPTH.with(std::cell::Cell::get);
    if media_type.eq_ignore_ascii_case("image/png") || media_type.eq_ignore_ascii_case("image/apng")
    {
        return decode_png_bytes(bytes, max_decoded_bytes);
    }
    if media_type.eq_ignore_ascii_case("image/jpeg") {
        return decode_jpeg_bytes(bytes, max_decoded_bytes);
    }
    if media_type.eq_ignore_ascii_case("image/webp") {
        return decode_webp_bytes(bytes, max_decoded_bytes);
    }
    if media_type.eq_ignore_ascii_case("image/gif") {
        return decode_gif_bytes(bytes, max_decoded_bytes);
    }
    if media_type.eq_ignore_ascii_case("image/svg+xml") {
        return decode_svg_bytes(bytes, max_decoded_bytes, decode_depth);
    }
    None
}

fn decode_svg_bytes(
    bytes: &[u8],
    max_decoded_bytes: usize,
    decode_depth: usize,
) -> Option<NativeImage> {
    if bytes.is_empty()
        || bytes.len() > MAX_NATIVE_IMAGE_BYTES
        || max_decoded_bytes < 4
        || decode_depth >= MAX_NATIVE_SVG_IMAGE_DECODE_DEPTH
    {
        return None;
    }
    let source = std::str::from_utf8(bytes).ok()?.trim();
    if source.is_empty() {
        return None;
    }
    let limits = NativeEngineLimits::default();
    let document = NativeDocument::parse(source, &limits).ok()?;
    if document_contains_nested_raster_image(&document) {
        return None;
    }
    let root = document
        .node(document.root())?
        .children()
        .iter()
        .find_map(|node_id| {
            let node = document.node(*node_id)?;
            (node.element_name() == Some("svg")
                && node.namespace_uri() == Some(super::dom::SVG_NAMESPACE_URI))
            .then_some(*node_id)
        })?;
    let (width, height) = svg_image_dimensions(document.node(root)?)?;
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let decoded_bytes = pixel_count.checked_mul(4)?;
    if decoded_bytes > max_decoded_bytes || pixel_count > super::raster::MAX_NATIVE_SURFACE_PIXELS {
        return None;
    }
    let surface = NATIVE_SVG_IMAGE_DECODE_DEPTH.with(|depth| {
        depth.set(decode_depth.saturating_add(1));
        let result = document.rasterize(Viewport {
            width,
            height,
            device_scale_factor_milli: 1000,
        });
        depth.set(decode_depth);
        result.ok()
    })?;
    (surface.width() == width
        && surface.height() == height
        && surface.rgba().len() == decoded_bytes)
        .then(|| NativeImage::new(width, height, surface.rgba().to_vec()))
}

fn document_contains_nested_raster_image(document: &NativeDocument) -> bool {
    fn contains(document: &NativeDocument, node_id: super::dom::NativeNodeId) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };
        let nested_svg_image = node.element_name() == Some("img")
            && is_svg_data_url(&document.image_current_src(node_id, Viewport::default()));
        if nested_svg_image
            || document
                .background_image_source_for_node(node_id)
                .is_some_and(is_svg_data_url)
        {
            return true;
        }
        node.children()
            .iter()
            .any(|child| contains(document, *child))
    }
    let Some(root) = document.node(document.root()) else {
        return false;
    };
    root.children()
        .iter()
        .any(|child| contains(document, *child))
}

fn is_svg_data_url(source: &str) -> bool {
    let Some(prefix) = source.get(..5) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case("data:") {
        return false;
    }
    let metadata = &source[5..];
    metadata
        .split_once(',')
        .and_then(|(metadata, _)| metadata.split(';').next())
        .is_some_and(|media_type| media_type.eq_ignore_ascii_case("image/svg+xml"))
}

fn svg_image_dimensions(node: &super::dom::NativeNode) -> Option<(u32, u32)> {
    let width = svg_length(node.attribute("width"));
    let height = svg_length(node.attribute("height"));
    let viewbox = node.attribute("viewBox").and_then(parse_svg_viewbox);
    let width = width.unwrap_or(DEFAULT_NATIVE_SVG_IMAGE_WIDTH);
    let height = height.unwrap_or(DEFAULT_NATIVE_SVG_IMAGE_HEIGHT);
    if width == 0 || height == 0 {
        return None;
    }
    if node.attribute("width").is_none()
        && node.attribute("height").is_some()
        && let Some((viewbox_width, viewbox_height)) = viewbox
    {
        let width = scaled_svg_dimension(height, viewbox_width, viewbox_height)?;
        return Some((width, height));
    }
    if node.attribute("height").is_none()
        && node.attribute("width").is_some()
        && let Some((viewbox_width, viewbox_height)) = viewbox
    {
        let height = scaled_svg_dimension(width, viewbox_height, viewbox_width)?;
        return Some((width, height));
    }
    Some((width, height))
}

fn svg_length(value: Option<&str>) -> Option<u32> {
    let value = value?.trim();
    let value = value.strip_suffix("px").map(str::trim).unwrap_or(value);
    if value.is_empty() || value.ends_with('%') {
        return None;
    }
    let value = value.parse::<f64>().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    u32::try_from(value.ceil() as u64)
        .ok()
        .filter(|value| *value > 0)
}

fn parse_svg_viewbox(value: &str) -> Option<(f64, f64)> {
    let values = value
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|value| !value.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() == 4
        && values.iter().all(|value| value.is_finite())
        && values[2] > 0.0
        && values[3] > 0.0)
        .then_some((values[2], values[3]))
}

fn scaled_svg_dimension(value: u32, numerator: f64, denominator: f64) -> Option<u32> {
    let scaled = f64::from(value) * numerator / denominator;
    if !scaled.is_finite() || scaled <= 0.0 {
        return None;
    }
    u32::try_from(scaled.ceil() as u64)
        .ok()
        .filter(|value| *value > 0)
}

fn decode_jpeg_bytes(bytes: &[u8], max_decoded_bytes: usize) -> Option<NativeImage> {
    if bytes.is_empty() || max_decoded_bytes < 4 {
        return None;
    }
    let max_pixels = (max_decoded_bytes / 4).min(MAX_NATIVE_IMAGE_BYTES / 4);
    let max_dimension = max_pixels.max(1);
    let options = DecoderOptions::new_safe()
        .set_max_width(max_dimension)
        .set_max_height(max_dimension)
        .jpeg_set_max_scans(64)
        .jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().ok()?;
    let (width, height) = decoder.dimensions()?;
    let pixel_count = usize::try_from(width).ok()?.checked_mul(height)?;
    if width == 0 || height == 0 || pixel_count > max_pixels {
        return None;
    }
    let decoded = decoder.decode().ok()?;
    let expected_bytes = pixel_count.checked_mul(4)?;
    if decoded.len() != expected_bytes || decoded.len() > max_decoded_bytes {
        return None;
    }
    Some(NativeImage::new(
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
        decoded,
    ))
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn decode_webp_bytes(bytes: &[u8], max_decoded_bytes: usize) -> Option<NativeImage> {
    if bytes.is_empty() || max_decoded_bytes < 4 {
        return None;
    }
    let mut decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
    decoder.set_memory_limit(max_decoded_bytes);
    if decoder.is_animated() {
        return decode_animated_webp(decoder, max_decoded_bytes);
    }
    let (width, height) = decoder.dimensions();
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let max_pixels = (max_decoded_bytes / 4).min(MAX_NATIVE_IMAGE_BYTES / 4);
    if width == 0 || height == 0 || pixel_count > max_pixels {
        return None;
    }
    let output_size = decoder.output_buffer_size()?;
    if output_size > max_decoded_bytes {
        return None;
    }
    let mut decoded = vec![0; output_size];
    decoder.read_image(&mut decoded).ok()?;
    let pixels: Vec<u8> = if decoder.has_alpha() {
        decoded
    } else {
        decoded
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], u8::MAX])
            .collect()
    };
    (pixels.len() == pixel_count.checked_mul(4)? && pixels.len() <= max_decoded_bytes)
        .then(|| NativeImage::new(width, height, pixels))
}

fn decode_animated_webp(
    mut decoder: WebPDecoder<Cursor<&[u8]>>,
    max_decoded_bytes: usize,
) -> Option<NativeImage> {
    let (width, height) = decoder.dimensions();
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let max_pixels = (max_decoded_bytes / 4).min(MAX_NATIVE_IMAGE_BYTES / 4);
    if width == 0 || height == 0 || pixel_count > max_pixels {
        return None;
    }
    let frame_count = usize::try_from(decoder.num_frames()).ok()?;
    if frame_count == 0 || frame_count > MAX_NATIVE_IMAGE_FRAMES {
        return None;
    }
    let canvas_bytes = pixel_count.checked_mul(4)?;
    let retained_bytes = frame_count.checked_add(1)?.checked_mul(canvas_bytes)?;
    if retained_bytes > max_decoded_bytes {
        return None;
    }
    let output_size = decoder.output_buffer_size()?;
    if output_size > max_decoded_bytes {
        return None;
    }
    let has_alpha = decoder.has_alpha();
    let loop_count = match decoder.loop_count() {
        LoopCount::Forever => None,
        LoopCount::Times(repeats) => Some(u32::from(repeats.get())),
    };
    let mut decoded = vec![0; output_size];
    let mut frames = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let delay_ms = decoder.read_frame(&mut decoded).ok()?;
        let pixels = if has_alpha {
            decoded.clone()
        } else {
            decoded
                .chunks_exact(3)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], u8::MAX])
                .collect()
        };
        if pixels.len() != canvas_bytes {
            return None;
        }
        frames.push(NativeImageFrame {
            delay_ms: normalized_webp_delay_ms(delay_ms),
            pixels,
        });
    }
    let image = NativeImage::with_frames(width, height, frames, loop_count)?;
    (image.decoded_bytes()? <= max_decoded_bytes).then_some(image)
}

fn normalized_webp_delay_ms(delay_ms: u32) -> u32 {
    delay_ms.max(1).min(MAX_NATIVE_IMAGE_FRAME_DELAY_MS)
}

fn decode_gif_bytes(bytes: &[u8], max_decoded_bytes: usize) -> Option<NativeImage> {
    if bytes.is_empty() || max_decoded_bytes < 4 {
        return None;
    }
    let memory_limit = MemoryLimit::Bytes(NonZeroU64::new(u64::try_from(max_decoded_bytes).ok()?)?);
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    options.set_memory_limit(memory_limit);
    options.check_frame_consistency(true);
    let mut decoder = options.read_info(Cursor::new(bytes)).ok()?;
    let width = u32::from(decoder.width());
    let height = u32::from(decoder.height());
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let max_pixels = (max_decoded_bytes / 4).min(MAX_NATIVE_IMAGE_BYTES / 4);
    if width == 0 || height == 0 || pixel_count > max_pixels {
        return None;
    }
    let repeat = decoder.repeat();
    let loop_count = match repeat {
        Repeat::Infinite => None,
        Repeat::Finite(repeats) => Some(u32::from(repeats)),
    };
    let canvas_bytes = pixel_count.checked_mul(4)?;
    let decoder_width = decoder.width();
    let decoder_height = decoder.height();
    let mut canvas = vec![0; canvas_bytes];
    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().ok()? {
        if frames.len() >= MAX_NATIVE_IMAGE_FRAMES
            || frame.width == 0
            || frame.height == 0
            || frame.left.checked_add(frame.width)? > decoder_width
            || frame.top.checked_add(frame.height)? > decoder_height
        {
            return None;
        }
        let frame_width = usize::from(frame.width);
        let frame_height = usize::from(frame.height);
        let frame_pixels = frame.buffer.to_vec();
        let frame_bytes = frame_width.checked_mul(frame_height)?.checked_mul(4)?;
        if frame_pixels.len() != frame_bytes {
            return None;
        }
        let delay_ms = normalized_gif_delay_ms(frame.delay);
        let disposal = frame.dispose;
        let left = usize::from(frame.left);
        let top = usize::from(frame.top);
        let previous = (disposal == DisposalMethod::Previous).then(|| canvas.clone());
        for (row, source_row) in frame_pixels.chunks_exact(frame_width * 4).enumerate() {
            let canvas_start = top
                .checked_add(row)?
                .checked_mul(usize::try_from(width).ok()?)?
                .checked_add(left)?
                .checked_mul(4)?;
            let canvas_row = canvas.get_mut(canvas_start..canvas_start + frame_width * 4)?;
            for (destination, source) in canvas_row
                .chunks_exact_mut(4)
                .zip(source_row.chunks_exact(4))
            {
                if source[3] != 0 {
                    destination.copy_from_slice(source);
                }
            }
        }
        let next_bytes = frames.len().saturating_add(1).checked_mul(canvas_bytes)?;
        if next_bytes > max_decoded_bytes {
            return None;
        }
        frames.push(NativeImageFrame {
            delay_ms,
            pixels: canvas.clone(),
        });
        match disposal {
            DisposalMethod::Background => {
                clear_gif_rect(&mut canvas, width, left, top, frame_width, frame_height)?;
            }
            DisposalMethod::Previous => {
                canvas = previous?;
            }
            DisposalMethod::Any | DisposalMethod::Keep => {}
        }
    }
    if frames.len() == 1 {
        return Some(NativeImage::new(width, height, frames.pop()?.pixels));
    }
    let image = NativeImage::with_frames(width, height, frames, loop_count)?;
    (image.decoded_bytes()? <= max_decoded_bytes).then_some(image)
}

fn normalized_gif_delay_ms(delay: u16) -> u32 {
    let delay_ms = if delay == 0 {
        100
    } else {
        u32::from(delay).saturating_mul(10)
    };
    delay_ms.min(MAX_NATIVE_IMAGE_FRAME_DELAY_MS)
}

fn clear_gif_rect(
    canvas: &mut [u8],
    canvas_width: u32,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
) -> Option<()> {
    let canvas_width = usize::try_from(canvas_width).ok()?;
    for row in top..top.checked_add(height)? {
        let start = row
            .checked_mul(canvas_width)?
            .checked_add(left)?
            .checked_mul(4)?;
        canvas
            .get_mut(start..start.checked_add(width.checked_mul(4)?)?)?
            .fill(0);
    }
    Some(())
}

fn percent_decode_bytes(value: &str) -> Option<Vec<u8>> {
    if value.len() > MAX_NATIVE_IMAGE_BYTES.saturating_mul(3) {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len().min(MAX_NATIVE_IMAGE_BYTES));
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index.saturating_add(2) >= bytes.len() {
                return None;
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index = index.saturating_add(3);
        } else {
            decoded.push(byte);
            index = index.saturating_add(1);
        }
        if decoded.len() > MAX_NATIVE_IMAGE_BYTES {
            return None;
        }
    }
    Some(decoded)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_jpeg_bytes;

    #[test]
    fn decode_progressive_jpeg_fixture() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/remote-android-concept.jpg"
        ));
        assert!(decode_jpeg_bytes(bytes, 512 * 1024).is_some());
    }
}
