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
