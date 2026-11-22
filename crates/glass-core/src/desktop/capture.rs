use std::{
    io::Read,
    process::Child,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use super::MAX_FRAME;

const MIN_STARTUP_FRAME_SEQUENCE: u64 = 6;

pub(super) struct Frame {
    pub(super) seq: u64,
    pub(super) since_start_ms: u64,
    pub(super) captured_at: Instant,
    pub(super) rgb: Vec<u8>,
    jpeg: OnceLock<Result<Vec<u8>, String>>,
}

#[derive(Default)]
pub(super) struct Frames {
    pub(super) current: Option<Arc<Frame>>,
    pub(super) error: Option<String>,
}

impl Frame {
    pub(super) fn jpeg(&self) -> Result<&[u8], String> {
        self.jpeg
            .get_or_init(|| {
                let mut jpeg = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80)
                    .encode(
                        &self.rgb,
                        super::WIDTH,
                        super::HEIGHT,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|_| "Desktop image encoding failed".to_owned())?;
                if jpeg.len() > MAX_FRAME {
                    return Err("Desktop JPEG frame exceeded size limit".into());
                }
                Ok(jpeg)
            })
            .as_ref()
            .map(Vec::as_slice)
            .map_err(Clone::clone)
    }
}

/// Crops a normalized region of a full-resolution frame and enlarges it up to
/// 4x, so small text can be read without a second full screenshot.
pub(super) fn zoom(rgb: &[u8], region: &serde_json::Value) -> Result<serde_json::Value, String> {
    use base64::Engine as _;
    let field = |name: &str| {
        region[name]
            .as_f64()
            .filter(|value| (0.0..=1.0).contains(value))
            .ok_or_else(|| format!("region.{name} must be a number between 0 and 1"))
    };
    if region.as_object().is_none_or(|object| object.len() != 4) {
        return Err("region must be {x, y, width, height}, each a fraction of the screen".into());
    }
    let (x, y, width, height) = (field("x")?, field("y")?, field("width")?, field("height")?);
    if width < 0.02 || height < 0.02 || x + width > 1.0001 || y + height > 1.0001 {
        return Err("region must be at least 0.02 wide and tall and stay inside the screen (x + width <= 1, y + height <= 1)".into());
    }
    let (screen_w, screen_h) = (super::WIDTH, super::HEIGHT);
    let left = ((x * screen_w as f64).floor() as u32).min(screen_w - 1);
    let top = ((y * screen_h as f64).floor() as u32).min(screen_h - 1);
    let crop_w = ((width * screen_w as f64).round() as u32).clamp(1, screen_w - left);
    let crop_h = ((height * screen_h as f64).round() as u32).clamp(1, screen_h - top);
    let frame = image::RgbImage::from_raw(screen_w, screen_h, rgb.to_vec())
        .ok_or("Desktop frame has an unexpected size")?;
    let crop = image::imageops::crop_imm(&frame, left, top, crop_w, crop_h).to_image();
    let scale = (screen_w as f64 / crop_w as f64)
        .min(screen_h as f64 / crop_h as f64)
        .clamp(1.0, 4.0);
    let zoomed = if scale > 1.0 {
        image::imageops::resize(
            &crop,
            (crop_w as f64 * scale) as u32,
            (crop_h as f64 * scale) as u32,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        crop
    };
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode(
            &zoomed,
            zoomed.width(),
            zoomed.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|_| "Desktop image encoding failed".to_owned())?;
    if jpeg.len() > MAX_FRAME {
        return Err("Desktop JPEG frame exceeded size limit".into());
    }
    let round = |value: f64| (value * 10_000.0).round() / 10_000.0;
    Ok(serde_json::json!({
        "jpeg": base64::engine::general_purpose::STANDARD.encode(jpeg),
        "region": {
            "x": round(left as f64 / screen_w as f64),
            "y": round(top as f64 / screen_h as f64),
            "width": round(crop_w as f64 / screen_w as f64),
            "height": round(crop_h as f64 / screen_h as f64)
        },
        "imageWidth": zoomed.width(),
        "imageHeight": zoomed.height()
    }))
}

pub(super) fn read_frames(mut input: impl Read, frames: Arc<Mutex<Frames>>, started: Instant) {
    // Always drain capture into one current raw frame. JPEG is lazy and cached
    // per frame; neither encoding nor a slow viewer can block this reader.
    let mut seq = 0u64;
    loop {
        let mut rgb = vec![0; (super::WIDTH * super::HEIGHT * 3) as usize];
        if input.read_exact(&mut rgb).is_err() {
            frames.lock().unwrap().error = Some("Desktop frame source exited".into());
            return;
        }
        seq = seq.saturating_add(1);
        let captured_at = Instant::now();
        let frame = Arc::new(Frame {
            seq,
            since_start_ms: captured_at
                .saturating_duration_since(started)
                .as_millis()
                .min(u64::MAX as u128) as u64,
            captured_at,
            rgb,
            jpeg: OnceLock::new(),
        });
        let mut state = frames.lock().unwrap();
        state.current = Some(frame);
        state.error = None;
    }
}

pub(super) fn wait_for_first_frame(frames: &Arc<Mutex<Frames>>, capture: &mut Child) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut checked_seq = 0;
    while Instant::now() < deadline {
        let current = frames.lock().unwrap().current.as_ref().and_then(|frame| {
            (frame.seq > checked_seq && frame.seq >= MIN_STARTUP_FRAME_SEQUENCE)
                .then(|| frame.clone())
        });
        if let Some(frame) = current {
            checked_seq = frame.seq;
            // FFmpeg can capture a fresh but empty Xvfb framebuffer before
            // XFCE paints. Wait for desktop-sized pixel coverage so Connect
            // does not report a black startup buffer as a usable live view.
            if frame.jpeg().is_ok_and(has_visible_desktop_pixels) {
                return true;
            }
        }
        if capture.try_wait().ok().flatten().is_some() {
            return false;
        }
        thread::sleep(Duration::from_millis(40));
    }
    false
}

fn has_visible_desktop_pixels(jpeg: &[u8]) -> bool {
    let Ok(image) = image::load_from_memory(jpeg) else {
        return false;
    };
    let image = image.to_rgb8();
    let step_x = (image.width() / 160).max(1) as usize;
    let step_y = (image.height() / 100).max(1) as usize;
    let mut samples = 0usize;
    let mut visible = 0usize;
    for y in (0..image.height()).step_by(step_y) {
        for x in (0..image.width()).step_by(step_x) {
            samples += 1;
            if image.get_pixel(x, y).0.iter().any(|channel| *channel > 24) {
                visible += 1;
            }
        }
    }
    visible.saturating_mul(100) > samples
}

pub(super) fn stop_child(child: &mut Child) {
    let pid = child.id() as i32;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut reaped = false;
    loop {
        if !reaped {
            reaped = child.try_wait().ok().flatten().is_some();
        }
        if reaped && !process_group_exists(pid) {
            return;
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.wait();
}

fn process_group_exists(pid: i32) -> bool {
    if unsafe { libc::kill(-pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_crops_the_region_at_full_resolution_and_rejects_bad_regions() {
        let rgb = vec![90; (super::super::WIDTH * super::super::HEIGHT * 3) as usize];
        let zoomed = zoom(
            &rgb,
            &serde_json::json!({"x":0.5,"y":0.5,"width":0.25,"height":0.25}),
        )
        .unwrap();
        assert_eq!(zoomed["imageWidth"], 1280);
        assert_eq!(zoomed["imageHeight"], 800);
        assert_eq!(zoomed["region"]["x"], 0.5);
        let error = |region: serde_json::Value| zoom(&rgb, &region).err().unwrap_or_default();
        assert!(
            error(serde_json::json!({"x":0.9,"y":0.0,"width":0.5,"height":0.5})).contains("inside")
        );
        assert!(
            error(serde_json::json!({"x":0.0,"y":0.0,"width":0.001,"height":0.5})).contains("0.02")
        );
        assert!(error(serde_json::json!({"x":0.0,"y":0.0})).contains("{x, y, width, height}"));
    }

    #[test]
    fn raw_reader_keeps_latest_complete_frame_and_jpeg_is_lazy() {
        let frame_size = (super::super::WIDTH * super::super::HEIGHT * 3) as usize;
        let mut input = vec![17; frame_size];
        input.extend(vec![91; frame_size]);
        input.extend([1, 2, 3]); // Partial third frame must not replace the second.
        let frames = Arc::new(Mutex::new(Frames::default()));
        read_frames(std::io::Cursor::new(input), frames.clone(), Instant::now());
        let frame = frames.lock().unwrap().current.clone().unwrap();
        assert_eq!(frame.seq, 2);
        assert!(frame.rgb.iter().all(|pixel| *pixel == 91));
        assert!(frame.jpeg.get().is_none());
        assert!(!frame.jpeg().unwrap().is_empty());
        assert!(frame.jpeg.get().is_some());
    }

    #[test]
    fn startup_frame_gate_rejects_black_xvfb_buffer() {
        let mut image = image::RgbImage::from_pixel(1280, 800, image::Rgb([0, 0, 0]));
        assert!(!has_visible_desktop_pixels(&encode_jpeg(&image)));
        for y in 0..48 {
            for x in 0..1280 {
                image.put_pixel(x, y, image::Rgb([48, 48, 48]));
            }
        }
        assert!(has_visible_desktop_pixels(&encode_jpeg(&image)));
    }

    fn encode_jpeg(image: &image::RgbImage) -> Vec<u8> {
        let mut jpeg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image.clone())
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        jpeg.into_inner()
    }
}
