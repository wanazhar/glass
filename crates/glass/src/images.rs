//! Lazy image preparation and durable, content-addressed model input.
use base64::Engine;
use glass_mod_api::Message;
use serde_json::{Value, json};
use std::{io::Cursor, sync::Mutex};

const PIXELS: u64 = 32_000_000;
const EDGE: u32 = 1600;
const ENCODED: usize = 1_048_576;
// Bound hot replay storage; raw image bytes never enter textual tool output.
static PREPARED: Mutex<Vec<(String, Value)>> = Mutex::new(Vec::new());
static HOT: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

pub fn prepare(bytes: &[u8], path: &str, cancelled: impl Fn() -> bool) -> Result<Value, String> {
    if cancelled() {
        return Err("Image operation cancelled".into());
    }
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("image exceeds 8 MiB".into());
    }
    let source_key = format!(
        "{}:{}",
        crate::config::data_dir().display(),
        crate::files::revision(bytes)
    );
    if let Ok(mut cache) = PREPARED.lock()
        && let Some(index) = cache.iter().position(|(key, _)| key == &source_key)
    {
        let mut item = cache.remove(index);
        let reference = item.1["reference"].as_str().unwrap_or_default();
        let root = crate::config::data_dir().join("images");
        let intact = crate::files::read_workspace_bytes(&root, &format!("{reference}.jpg"))
            .is_ok_and(|bytes| {
                bytes.len() <= ENCODED && crate::files::revision(&bytes) == reference
            });
        if intact {
            item.1["path"] = json!(path);
            let result = item.1.clone();
            cache.push(item);
            return Ok(result);
        }
        // Evict stale preparation metadata so a repeated call can repair a
        // missing/corrupt artifact rather than returning the same broken ref.
    }
    let format = image::guess_format(bytes).map_err(|e| format!("invalid image: {e}"))?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP
    ) {
        return Err("supported images: PNG, JPEG, GIF, WebP".into());
    }
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| format!("invalid image dimensions: {e}"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > PIXELS {
        return Err("image exceeds 32 million pixels or has empty dimensions".into());
    }
    if cancelled() {
        return Err("Image operation cancelled".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32_000);
    limits.max_image_height = Some(32_000);
    limits.max_alloc = Some(192 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("invalid image pixels: {e}"))?;
    if cancelled() {
        return Err("Image operation cancelled".into());
    }
    let small = if width > EDGE || height > EDGE {
        decoded.thumbnail(EDGE, EDGE).to_rgba8()
    } else {
        decoded.to_rgba8()
    };
    drop(decoded);
    let mut pixels = image::RgbImage::new(small.width(), small.height());
    for (source, target) in small.pixels().zip(pixels.pixels_mut()) {
        let alpha = u16::from(source[3]);
        for channel in 0..3 {
            target[channel] =
                ((u16::from(source[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8;
        }
    }
    drop(small);
    let mut encoded = Vec::new();
    loop {
        encoded.clear();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
            .encode_image(&pixels)
            .map_err(|e| e.to_string())?;
        if encoded.len() <= ENCODED {
            break;
        }
        if cancelled() {
            return Err("Image operation cancelled".into());
        }
        pixels = image::imageops::resize(
            &pixels,
            (pixels.width() / 2).max(1),
            (pixels.height() / 2).max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    if cancelled() {
        return Err("Image operation cancelled".into());
    }
    let reference = crate::files::revision(&encoded);
    let target = crate::config::data_dir()
        .join("images")
        .join(format!("{reference}.jpg"));
    crate::auth::secure_write(&target, &encoded)?;
    let result = json!({"kind":"image_view", "reference":reference, "path":path,
        "width":width,"height":height,"sent_width":pixels.width(),"sent_height":pixels.height(),
        "bytes":encoded.len(),"media_type":"image/jpeg"});
    if let Ok(mut cache) = PREPARED.lock() {
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((source_key, result.clone()));
    }
    Ok(result)
}

pub fn display(value: &Value) -> String {
    format!(
        "[image {} · {} · {}×{} → {}×{} · {} KiB]\nPixels supplied to the model; metadata stripped. GIF/WebP uses the first frame.",
        value["reference"].as_str().unwrap_or_default(),
        value["path"].as_str().unwrap_or_default(),
        value["width"],
        value["height"],
        value["sent_width"],
        value["sent_height"],
        value["bytes"].as_u64().unwrap_or(0).div_ceil(1024)
    )
}

/// Validate restored legacy snapshots without decoding their pixel buffers.
pub fn valid_snapshot(media_type: &str, data: &str) -> bool {
    if data.len() > ENCODED.div_ceil(3) * 4 {
        return false;
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return false;
    };
    if bytes.len() > ENCODED || crate::polish::image_media_type(&bytes) != Some(media_type) {
        return false;
    }
    let Ok(reader) = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format() else {
        return false;
    };
    reader.into_dimensions().is_ok_and(|(width, height)| {
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= PIXELS
    })
}

fn replay(body: &str) -> Result<Message, String> {
    let reference = body
        .strip_prefix("[image ")
        .and_then(|v| v.split_once(" · "))
        .map(|v| v.0)
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("invalid image reference")?;
    let mut hot = HOT.lock().map_err(|_| "image cache unavailable")?;
    if let Some(index) = hot.iter().position(|(key, _)| key == reference) {
        let item = hot.remove(index);
        let data = item.1.clone();
        hot.push(item);
        return Ok(Message::Image {
            media_type: "image/jpeg".into(),
            data,
        });
    }
    let root = crate::config::data_dir().join("images");
    let bytes = crate::files::read_workspace_bytes(&root, &format!("{reference}.jpg"))?;
    if bytes.len() > ENCODED || crate::files::revision(&bytes) != reference {
        return Err("image artifact failed integrity check".into());
    }
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    if hot.len() >= 8 {
        hot.remove(0);
    }
    hot.push((reference.into(), data.clone()));
    Ok(Message::Image {
        media_type: "image/jpeg".into(),
        data,
    })
}

/// Insert image inputs only after a complete tool-result batch, never between
/// parallel calls and their results. Missing artifacts become explicit facts.
pub fn expand(messages: Vec<Message>) -> Vec<Message> {
    let latest: std::collections::HashMap<&str, usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            if let Message::ToolResult { name, body, .. } = message
                && name == "view_image"
                && body.starts_with("[image ")
            {
                body.split_once(" · ")
                    .map(|(reference, _)| (reference, index))
            } else {
                None
            }
        })
        .collect();
    let indices: std::collections::HashSet<usize> = latest.into_values().collect();
    let mut result = Vec::new();
    let mut pending = Vec::new();
    for (index, message) in messages.into_iter().enumerate() {
        if !matches!(message, Message::ToolResult { .. }) && !pending.is_empty() {
            result.append(&mut pending);
        }
        if let Message::ToolResult { name, body, .. } = &message
            && name == "view_image"
            && body.starts_with("[image ")
            && indices.contains(&index)
        {
            pending.push(replay(body).unwrap_or_else(|e| {
                Message::User(format!("Image unavailable: {e}. Call view_image again."))
            }));
        }
        result.push(message);
    }
    result.append(&mut pending);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(width, height)
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }
    #[test]
    fn preparation_is_bounded_and_replay_is_after_all_results() {
        let value = prepare(&png(2000, 4), "shot.png", || false).unwrap();
        assert_eq!(value["width"], 2000);
        assert!(value["sent_width"].as_u64().unwrap() <= 1600);
        assert!(value["bytes"].as_u64().unwrap() <= ENCODED as u64);
        // Simulate stale cached metadata without removing any durable artifact.
        let source_key = format!(
            "{}:{}",
            crate::config::data_dir().display(),
            crate::files::revision(&png(2000, 4))
        );
        if let Some((_, cached)) = PREPARED
            .lock()
            .unwrap()
            .iter_mut()
            .find(|(key, _)| key == &source_key)
        {
            cached["reference"] = json!("0".repeat(64));
        }
        assert_eq!(
            prepare(&png(2000, 4), "shot.png", || false).unwrap()["reference"],
            value["reference"]
        );
        let body = display(&value);
        let result = expand(vec![
            Message::ToolResult {
                call_id: "one".into(),
                name: "view_image".into(),
                body,
            },
            Message::ToolResult {
                call_id: "two".into(),
                name: "read".into(),
                body: "file".into(),
            },
            Message::Assistant("done".into()),
        ]);
        assert!(matches!(result[1], Message::ToolResult { .. }));
        assert!(matches!(result[2], Message::Image { .. }));
        assert!(matches!(result[3], Message::Assistant(_)));
    }
    #[test]
    fn invalid_cancelled_and_missing_images_are_explicit() {
        assert!(prepare(b"\x89PNG\r\n\x1a\n", "bad.png", || false).is_err());
        assert!(prepare(&png(2, 2), "shot.png", || true).is_err());
        let result = expand(vec![Message::ToolResult {
            call_id: "x".into(),
            name: "view_image".into(),
            body: "[image ../../escape · x]".into(),
        }]);
        assert!(matches!(&result[1], Message::User(text) if text.contains("unavailable")));
    }
}
