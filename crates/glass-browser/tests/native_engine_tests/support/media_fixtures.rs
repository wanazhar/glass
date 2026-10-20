use super::*;

pub(super) fn native_test_color_font_bytes() -> Vec<u8> {
    include_bytes!("../../fixtures/colr-v0.ttf").to_vec()
}

pub(super) fn native_test_png_bytes() -> Vec<u8> {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 0,
            ])
            .unwrap();
    }
    encoded
}

pub(super) fn native_test_solid_png_bytes(color: [u8; 4]) -> Vec<u8> {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&color.repeat(4)).unwrap();
    }
    encoded
}

pub(super) fn native_test_wav_bytes() -> Vec<u8> {
    let mut wav = vec![0_u8; 44 + 320];
    wav[0..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&(356_u32).to_le_bytes());
    wav[8..12].copy_from_slice(b"WAVE");
    wav[12..16].copy_from_slice(b"fmt ");
    wav[16..20].copy_from_slice(&(16_u32).to_le_bytes());
    wav[20..22].copy_from_slice(&(1_u16).to_le_bytes());
    wav[22..24].copy_from_slice(&(1_u16).to_le_bytes());
    wav[24..28].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[28..32].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[32..34].copy_from_slice(&(1_u16).to_le_bytes());
    wav[34..36].copy_from_slice(&(8_u16).to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&(320_u32).to_le_bytes());
    wav[44..].fill(128);
    wav
}

pub(super) fn native_test_file_url(path: &std::path::Path) -> String {
    Url::from_file_path(path).unwrap().to_string()
}

pub(super) fn native_test_png_data_url() -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_png_bytes())
    )
}

pub(super) fn native_test_jpeg_bytes() -> Vec<u8> {
    include_bytes!("../../fixtures/remote-android-concept.jpg").to_vec()
}

pub(super) fn native_test_small_jpeg_data_url() -> String {
    "data:image/jpeg;base64,/9j/4AAQSkZJRgABAgAAAQABAAD//gAPTGF2YzYzLjEuMTAxAP/bAEMACAQEBAQEBQUFBQUFBgYGBgYGBgYGBgYGBgcHBwgICAcHBwYGBwcICAgICQkJCAgICAkJCgoKDAwLCw4ODhERFP/EAEwAAQEAAAAAAAAAAAAAAAAAAAAGAQEBAAAAAAAAAAAAAAAAAAAGBxABAAAAAAAAAAAAAAAAAAAAABEBAAAAAAAAAAAAAAAAAAAAAP/AABEIAAIAAgMBIgACEQADEQD/2gAMAwEAAhEDEQA/AIsATX9//9k=".to_owned()
}

pub(super) fn native_test_webp_bytes() -> Vec<u8> {
    vec![
        0x52, 0x49, 0x46, 0x46, 0x3c, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x20, 0x30, 0x00, 0x00, 0x00, 0xd0, 0x01, 0x00, 0x9d, 0x01, 0x2a, 0x02, 0x00, 0x02, 0x00,
        0x02, 0x00, 0x34, 0x25, 0xa0, 0x02, 0x74, 0xba, 0x01, 0xf8, 0x00, 0x03, 0xb0, 0x00, 0xfe,
        0xf0, 0xc4, 0x0b, 0xff, 0x20, 0xb9, 0x61, 0x75, 0xc8, 0xd7, 0xff, 0x20, 0x3f, 0xe4, 0x07,
        0xfc, 0x80, 0xff, 0xf8, 0xf2, 0x00, 0x00, 0x00,
    ]
}

pub(super) fn native_test_small_webp_data_url() -> String {
    format!(
        "data:image/webp;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_webp_bytes())
    )
}

pub(super) fn native_test_gif_bytes() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode("R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==")
        .unwrap()
}

pub(super) fn native_test_small_gif_data_url() -> String {
    format!(
        "data:image/gif;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_gif_bytes())
    )
}

pub(super) fn native_test_animated_gif_bytes() -> Vec<u8> {
    let mut encoded = Vec::new();
    let palette = [255, 0, 0, 0, 255, 0];
    {
        let mut encoder = Encoder::new(&mut encoded, 1, 1, &palette).unwrap();
        encoder.set_repeat(Repeat::Infinite).unwrap();
        let first = Frame {
            delay: 1,
            width: 1,
            height: 1,
            buffer: Cow::Owned(vec![0]),
            ..Frame::default()
        };
        encoder.write_frame(&first).unwrap();
        let second = Frame {
            delay: 1,
            width: 1,
            height: 1,
            buffer: Cow::Owned(vec![1]),
            ..Frame::default()
        };
        encoder.write_frame(&second).unwrap();
    }
    encoded
}

pub(super) fn native_test_animated_webp_bytes() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode("UklGRoQAAABXRUJQVlA4WAoAAAACAAAAAAAAAAAAQU5JTQYAAAD/////AABBTk1GKAAAAAAAAAAAAAAAAAAAAAoAAAJWUDhMDwAAAC8AAAAABxD9j/4HIqL/AQBBTk1GKAAAAAAAAAAAAAAAAAAAAAoAAABWUDhMDwAAAC8AAAAAB9D/iP4HIqL/AQA=")
        .unwrap()
}

pub(super) fn native_test_animated_apng_bytes() -> Vec<u8> {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(2, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.set_frame_delay(1, 100).unwrap();
        writer.write_image_data(&[255, 0, 0, 255]).unwrap();
        writer.set_frame_delay(1, 100).unwrap();
        writer.write_image_data(&[0, 255, 0, 255]).unwrap();
        writer.finish().unwrap();
    }
    encoded
}

pub(super) fn native_test_svg_bytes() -> Vec<u8> {
    br#"<svg xmlns="http://www.w3.org/2000/svg" width="4px" height="3px" viewBox="0 0 4 3"><rect x="0" y="0" width="4" height="3" fill="red"></rect><circle cx="2" cy="1" r="1" fill="blue"></circle></svg>"#.to_vec()
}

pub(super) fn native_test_svg_data_url() -> String {
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_svg_bytes())
    )
}
