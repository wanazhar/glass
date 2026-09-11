use base64::Engine as _;
use png::ColorType;
use std::io::Cursor;

/// Maximum decoded RGBA bytes retained for one native inline image.
pub(crate) const MAX_NATIVE_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum decoded pixels retained for one native inline image.
pub(crate) const MAX_NATIVE_IMAGE_PIXELS: usize = MAX_NATIVE_IMAGE_BYTES / 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

pub(crate) fn decode_data_image(source: &str) -> Option<NativeImage> {
    let prefix = source.get(..5)?;
    if !prefix.eq_ignore_ascii_case("data:") {
        return None;
    }
    let (metadata, payload) = source.get(5..)?.split_once(',')?;
    let mut metadata_parts = metadata.split(';');
    let media_type = metadata_parts.next().unwrap_or_default();
    if !media_type.eq_ignore_ascii_case("image/png") {
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
    decode_png(&bytes)
}

pub(crate) fn image_dimensions_from_source(source: &str) -> Option<(u32, u32)> {
    decode_data_image(source).map(|image| (image.width, image.height))
}

fn decode_png(bytes: &[u8]) -> Option<NativeImage> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let width = reader.info().width;
    let height = reader.info().height;
    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    if width == 0 || height == 0 || pixel_count > MAX_NATIVE_IMAGE_PIXELS {
        return None;
    }
    let output_size = reader.output_buffer_size();
    if output_size > MAX_NATIVE_IMAGE_BYTES {
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
    (pixels.len() == pixel_count.checked_mul(4)?).then_some(NativeImage {
        width,
        height,
        pixels,
    })
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
