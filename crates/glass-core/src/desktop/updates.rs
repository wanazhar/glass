//! Session-scoped region updates with 32 bounded shared frame checkpoints.
//! A viewer acknowledges a frame sequence; unchanged pixels stay local.
//! There is no send queue or retained per-viewer state.
use base64::Engine;
use image::ImageEncoder;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::VecDeque, io::Cursor};

const TILE: u32 = 64;
const MAX_TILES: usize = 260;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    pub session_id: String,
    pub after: u64,
}
impl Request {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.session_id.is_empty() || self.session_id.len() > 128 {
            return Err("Invalid desktop update checkpoint".into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Checkpoints(VecDeque<(String, u64, Vec<String>)>);
impl Checkpoints {
    pub(super) fn get(&self, session: &str, sequence: u64) -> Vec<String> {
        self.0
            .iter()
            .find(|(id, seq, _)| id == session && *seq == sequence)
            .map(|(_, _, hashes)| hashes.clone())
            .unwrap_or_default()
    }
    pub(super) fn insert(&mut self, session: &str, sequence: u64, hashes: Vec<String>) {
        if self
            .0
            .iter()
            .any(|(id, seq, _)| id == session && *seq == sequence)
        {
            return;
        }
        if self.0.len() >= 32 {
            self.0.pop_front();
        }
        self.0.push_back((session.into(), sequence, hashes));
    }
}

pub(super) fn encode(
    session: &str,
    seq: u64,
    time_ms: u64,
    rgb: &[u8],
    previous: &[String],
) -> Result<(Value, Vec<String>), String> {
    if !(previous.is_empty() || previous.len() == MAX_TILES) {
        return Err("Invalid desktop image checkpoint".into());
    }
    let (width, height) = (super::WIDTH, super::HEIGHT);
    let image = image::RgbImage::from_raw(width, height, rgb.to_vec())
        .ok_or("Desktop update dimensions changed")?;
    let columns = width.div_ceil(TILE);
    let mut hashes = Vec::new();
    let mut changed = Vec::new();
    for y in (0..height).step_by(TILE as usize) {
        for x in (0..width).step_by(TILE as usize) {
            let w = TILE.min(width - x);
            let h = TILE.min(height - y);
            let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
            for row in y..y + h {
                let start = ((row * width + x) * 3) as usize;
                digest.update(&image.as_raw()[start..start + (w * 3) as usize]);
            }
            let hash: String = digest.finish().as_ref()[..8]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let index = hashes.len();
            if previous.get(index) != Some(&hash) {
                changed.push((index, x, y, w, h));
            }
            hashes.push(hash);
        }
    }
    let base64 = &base64::engine::general_purpose::STANDARD;
    let mut tiles = Vec::new();
    let mut encoded_bytes = 0;
    // Full refreshes avoid spending CPU/bytes encoding every tile separately.
    let mut full = previous.len() != hashes.len() || changed.len() > hashes.len() / 2;
    if !full {
        for (index, x, y, w, h) in &changed {
            let cropped = image::imageops::crop_imm(&image, *x, *y, *w, *h).to_image();
            let mut png = Cursor::new(Vec::new());
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(cropped.as_raw(), *w, *h, image::ExtendedColorType::Rgb8)
                .map_err(|_| "Encoding desktop region failed")?;
            // Text and flat UI tiles remain lossless. Textured tiles can be
            // much larger as PNG than the full JPEG they were cropped from.
            let mut encoded = png.into_inner();
            let mut mime = "image/png";
            if encoded.len() > 2048 {
                let mut jpeg_tile = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_tile, 85)
                    .encode(cropped.as_raw(), *w, *h, image::ExtendedColorType::Rgb8)
                    .map_err(|_| "Encoding desktop region failed")?;
                if jpeg_tile.len() < encoded.len() {
                    encoded = jpeg_tile;
                    mime = "image/jpeg";
                }
            }
            encoded_bytes += encoded.len();
            if encoded_bytes >= 256 * 1024 {
                full = true;
                tiles.clear();
                break;
            }
            tiles.push(json!({"index":index,"x":x,"y":y,"width":w,"height":h,"hash":hashes[*index],"mime":mime,"data":base64.encode(&encoded)}));
        }
    }
    let jpeg = if full {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80)
            .encode(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|_| "Encoding desktop refresh failed")?;
        Some(base64.encode(bytes))
    } else {
        None
    };
    let value = json!({
        "version":1,"sessionId":session,"frameSeq":seq,"frameTimeMs":time_ms,
        "width":width,"height":height,"tileSize":TILE,"columns":columns,"hashes":if full {Some(hashes.clone())} else {None},
        "full":jpeg,"tiles":tiles
    });
    Ok((value, hashes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn region_updates_send_only_changed_pixels_and_recover_without_checkpoint() {
        let mut scene =
            image::RgbImage::from_fn(super::super::WIDTH, super::super::HEIGHT, |x, y| {
                image::Rgb([
                    (x.wrapping_mul(13) + y * 3) as u8,
                    (y * 7) as u8,
                    (x + y) as u8,
                ])
            });
        let (first, hashes) = encode("session", 1, 100, scene.as_raw(), &[]).unwrap();
        assert!(first["full"].is_string());
        let (same, _) = encode("session", 2, 300, scene.as_raw(), &hashes).unwrap();
        assert!(same["full"].is_null());
        assert!(same["hashes"].is_null());
        assert_eq!(same["tiles"].as_array().unwrap().len(), 0);
        assert!(serde_json::to_vec(&same).unwrap().len() < 300);
        for y in 32..48 {
            for x in 32..48 {
                scene.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        let (changed, _) = encode("session", 3, 500, scene.as_raw(), &hashes).unwrap();
        assert!(changed["full"].is_null());
        let tiles = changed["tiles"].as_array().unwrap();
        assert!(!tiles.is_empty());
        assert!(tiles.len() <= 4);
        assert_eq!(tiles[0]["mime"], "image/jpeg");
        assert!(tiles[0]["data"].as_str().unwrap().len() < 6000);
        assert!(
            serde_json::to_vec(&changed).unwrap().len()
                < serde_json::to_vec(&first).unwrap().len() / 8
        );
        let region = base64::engine::general_purpose::STANDARD
            .decode(tiles[0]["data"].as_str().unwrap())
            .unwrap();
        let decoded = image::load_from_memory(&region).unwrap();
        assert_eq!(decoded.width(), 64);
        assert!(encode("next", 4, 700, scene.as_raw(), &[]).unwrap().0["full"].is_string());
    }
    #[test]
    fn region_updates_checkpoints_are_bounded_and_never_cross_sessions() {
        let mut cache = Checkpoints::default();
        for seq in 1..=40 {
            cache.insert("first", seq, vec![format!("{seq:016x}"); MAX_TILES]);
        }
        assert_eq!(cache.0.len(), 32);
        assert!(cache.get("first", 1).is_empty());
        assert!(cache.get("other", 40).is_empty());
        assert_eq!(cache.get("first", 40).len(), MAX_TILES);
        assert!(
            Request {
                session_id: String::new(),
                after: 0
            }
            .validate()
            .is_err()
        );
        assert!(
            serde_json::from_str::<Request>(r#"{"sessionId":"s","after":0,"hashes":[]}"#).is_err()
        );
    }
}
