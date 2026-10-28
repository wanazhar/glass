use std::{
    io::Read,
    process::Child,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::MAX_FRAME;

const MIN_STARTUP_FRAME_SEQUENCE: u64 = 6;

pub(super) struct Frame {
    pub(super) seq: u64,
    pub(super) since_start_ms: u64,
    pub(super) captured_at: Instant,
    pub(super) jpeg: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Frames {
    pub(super) current: Option<Frame>,
    pub(super) error: Option<String>,
}

pub(super) fn read_frames(mut input: impl Read, frames: Arc<Mutex<Frames>>, started: Instant) {
    let mut pending = Vec::with_capacity(256 * 1024);
    let mut chunk = [0u8; 32 * 1024];
    let mut seq = 0u64;
    loop {
        let count = match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                frames.lock().unwrap().error =
                    Some(format!("Desktop frame source failed: {error}"));
                return;
            }
        };
        pending.extend_from_slice(&chunk[..count]);
        loop {
            let Some(begin) = pending.windows(2).position(|pair| pair == [0xff, 0xd8]) else {
                if pending.len() > 1 {
                    pending.drain(..pending.len() - 1);
                }
                break;
            };
            if begin > 0 {
                pending.drain(..begin);
            }
            let Some(end) = pending.windows(2).position(|pair| pair == [0xff, 0xd9]) else {
                if pending.len() > MAX_FRAME {
                    pending.clear();
                    frames.lock().unwrap().error =
                        Some("Desktop JPEG frame exceeded size limit".into());
                }
                break;
            };
            let end = end + 2;
            if end > MAX_FRAME {
                pending.drain(..end);
                continue;
            }
            seq = seq.saturating_add(1);
            let jpeg = pending.drain(..end).collect::<Vec<_>>();
            let mut state = frames.lock().unwrap();
            state.current = Some(Frame {
                seq,
                since_start_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                captured_at: Instant::now(),
                jpeg,
            });
            state.error = None;
        }
    }
    let mut state = frames.lock().unwrap();
    if state.error.is_none() {
        state.error = Some("Desktop frame source exited".into());
    }
}

pub(super) fn wait_for_first_frame(frames: &Arc<Mutex<Frames>>, capture: &mut Child) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut checked_seq = 0;
    while Instant::now() < deadline {
        let current = frames.lock().unwrap().current.as_ref().and_then(|frame| {
            (frame.seq > checked_seq && frame.seq >= MIN_STARTUP_FRAME_SEQUENCE)
                .then(|| (frame.seq, frame.jpeg.clone()))
        });
        if let Some((seq, jpeg)) = current {
            checked_seq = seq;
            // FFmpeg can capture a fresh but empty Xvfb framebuffer before
            // XFCE paints. Wait for desktop-sized pixel coverage so Connect
            // does not report a black startup buffer as a usable live view.
            if has_visible_desktop_pixels(&jpeg) {
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
    use std::io::Cursor;

    #[test]
    fn frame_reader_keeps_latest_complete_frame_only() {
        let input = [
            0xff, 0xd8, 1, 0xff, 0xd9, // frame one
            0xff, 0xd8, 2, 3, 0xff, 0xd9, // frame two
        ];
        let frames = Arc::new(Mutex::new(Frames::default()));
        read_frames(Cursor::new(input), frames.clone(), Instant::now());
        let frames = frames.lock().unwrap();
        let current = frames.current.as_ref().unwrap();
        assert_eq!(current.seq, 2);
        assert_eq!(current.jpeg, [0xff, 0xd8, 2, 3, 0xff, 0xd9]);
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
