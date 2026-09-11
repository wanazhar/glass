use base64::Engine as _;
use image_webp::WebPDecoder;
use png::ColorType;
use std::io::Cursor;
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
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
    if !matches_ignore_ascii_case(media_type, &["image/png", "image/jpeg", "image/webp"]) {
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
    let pixels = match output.color_type {
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
    };
    (pixels.len() == pixel_count.checked_mul(4)? && pixels.len() <= max_decoded_bytes).then_some(
        NativeImage {
            width,
            height,
            pixels,
        },
    )
}

pub(crate) fn decode_image_bytes(
    bytes: &[u8],
    media_type: &str,
    max_decoded_bytes: usize,
) -> Option<NativeImage> {
    if media_type.eq_ignore_ascii_case("image/png") {
        return decode_png_bytes(bytes, max_decoded_bytes);
    }
    if media_type.eq_ignore_ascii_case("image/jpeg") {
        return decode_jpeg_bytes(bytes, max_decoded_bytes);
    }
    if media_type.eq_ignore_ascii_case("image/webp") {
        return decode_webp_bytes(bytes, max_decoded_bytes);
    }
    None
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
    Some(NativeImage {
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        pixels: decoded,
    })
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
        return None;
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
    (pixels.len() == pixel_count.checked_mul(4)? && pixels.len() <= max_decoded_bytes).then_some(
        NativeImage {
            width,
            height,
            pixels,
        },
    )
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
